//! Browser extension support on top of WebView2's profile extension API.
//!
//! Extensions live unpacked in `%LOCALAPPDATA%\LightBrowser\extensions\<folder>`. They are (re)added
//! to the WebView2 profile at startup, can be toggled/removed from the extensions page, and can be
//! installed from the Chrome Web Store (CRX download + unzip).

use std::io::Cursor;
use std::path::{Path, PathBuf};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{
    take_pwstr, BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
    ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler,
};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use wry::{WebView, WebViewExtWindows};

pub enum ExtEvent {
    /// An extension folder was added to the profile.
    Loaded { id: String, folder: PathBuf },
    LoadFailed { folder: PathBuf, error: String },
    /// Current extensions known to the profile: (id, name, enabled).
    List(Vec<(String, String, bool)>),
    /// Something changed (enabled/removed); the list should be refreshed.
    Changed,
    /// A Web Store download finished: the unpacked folder, or an error message.
    Downloaded(Result<PathBuf, String>),
}

/// Components WebView2 ships itself (clipboard helper, PDF viewer); hidden from the UI so they
/// can't be removed by accident.
pub fn is_builtin(id: &str) -> bool {
    matches!(id, "dgiklkfkllikcanfonkcabmbdfmgleag" | "mhjfbmdgcfjbbpaeojofohoefgiehjai")
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join("LightBrowser").join("extensions");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn profile(wv: &WebView) -> windows::core::Result<ICoreWebView2Profile7> {
    unsafe { wv.webview().cast::<ICoreWebView2_13>()?.Profile()?.cast::<ICoreWebView2Profile7>() }
}

fn ext_id(ext: &ICoreWebView2BrowserExtension) -> String {
    let mut p = PWSTR::null();
    unsafe {
        let _ = ext.Id(&mut p);
    }
    take_pwstr(p)
}

/// Add one unpacked extension folder to the profile.
pub fn add<F: Fn(ExtEvent) + Clone + 'static>(wv: &WebView, folder: &Path, notify: F) {
    let folder = folder.to_path_buf();
    let fail = |n: &F, f: &PathBuf, e: String| n(ExtEvent::LoadFailed { folder: f.clone(), error: e });
    let profile = match profile(wv) {
        Ok(p) => p,
        Err(e) => return fail(&notify, &folder, format!("extensions unavailable: {e}")),
    };
    let n2 = notify.clone();
    let f2 = folder.clone();
    let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |res, ext| {
        match (res, ext) {
            (Ok(()), Some(ext)) => n2(ExtEvent::Loaded { id: ext_id(&ext), folder: f2 }),
            (Err(e), _) => n2(ExtEvent::LoadFailed { folder: f2, error: e.message().to_string() }),
            _ => n2(ExtEvent::LoadFailed { folder: f2, error: "unknown error".into() }),
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = profile.AddBrowserExtension(&HSTRING::from(folder.as_path()), &handler) {
            fail(&notify, &folder, e.message().to_string());
        }
    }
}

/// Add every extension folder (one containing a manifest.json) in the extensions directory.
pub fn load_all<F: Fn(ExtEvent) + Clone + 'static>(wv: &WebView, notify: F) {
    let Ok(entries) = std::fs::read_dir(dir()) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.join("manifest.json").is_file() {
            add(wv, &p, notify.clone());
        }
    }
}

fn with_extensions<F>(wv: &WebView, f: F)
where
    F: FnOnce(Vec<ICoreWebView2BrowserExtension>) + 'static,
{
    let Ok(profile) = profile(wv) else { return };
    let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |_, list| {
        let mut out = vec![];
        if let Some(list) = list {
            let mut n = 0u32;
            unsafe {
                let _ = list.Count(&mut n);
                for i in 0..n {
                    if let Ok(ext) = list.GetValueAtIndex(i) {
                        out.push(ext);
                    }
                }
            }
        }
        f(out);
        Ok(())
    }));
    unsafe {
        let _ = profile.GetBrowserExtensions(&handler);
    }
}

pub fn list<F: Fn(ExtEvent) + 'static>(wv: &WebView, notify: F) {
    with_extensions(wv, move |exts| {
        let items = exts
            .iter()
            .map(|ext| {
                let mut name = PWSTR::null();
                let mut on = BOOL(0);
                unsafe {
                    let _ = ext.Name(&mut name);
                    let _ = ext.IsEnabled(&mut on);
                }
                (ext_id(ext), take_pwstr(name), on.as_bool())
            })
            .collect();
        notify(ExtEvent::List(items));
    });
}

pub fn set_enabled<F: Fn(ExtEvent) + 'static>(wv: &WebView, id: String, on: bool, notify: F) {
    with_extensions(wv, move |exts| {
        if let Some(ext) = exts.into_iter().find(|e| ext_id(e) == id) {
            let h = BrowserExtensionEnableCompletedHandler::create(Box::new(move |_| {
                notify(ExtEvent::Changed);
                Ok(())
            }));
            unsafe {
                let _ = ext.Enable(on, &h);
            }
        }
    });
}

/// Remove from the profile and delete its folder (only if it's inside our extensions directory).
pub fn remove<F: Fn(ExtEvent) + 'static>(wv: &WebView, id: String, folder: Option<PathBuf>, notify: F) {
    with_extensions(wv, move |exts| {
        if let Some(ext) = exts.into_iter().find(|e| ext_id(e) == id) {
            let h = BrowserExtensionRemoveCompletedHandler::create(Box::new(move |_| {
                if let Some(f) = folder.filter(|f| f.starts_with(dir())) {
                    let _ = std::fs::remove_dir_all(f);
                }
                notify(ExtEvent::Changed);
                Ok(())
            }));
            unsafe {
                let _ = ext.Remove(&h);
            }
        }
    });
}

/// Popup page and options page paths from an extension's manifest.
pub fn manifest_pages(folder: &Path) -> (Option<String>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(folder.join("manifest.json")) else { return (None, None) };
    let Ok(m) = serde_json::from_str::<serde_json::Value>(&text) else { return (None, None) };
    let s = |v: &serde_json::Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_string);
    let popup = s(&m["action"]["default_popup"]).or_else(|| s(&m["browser_action"]["default_popup"]));
    let options = s(&m["options_ui"]["page"]).or_else(|| s(&m["options_page"]));
    (popup, options)
}

/// Extract a Chrome Web Store extension ID from a store URL or a bare ID.
pub fn store_id(input: &str) -> Option<String> {
    let is_id = |s: &str| s.len() == 32 && s.bytes().all(|b| (b'a'..=b'p').contains(&b));
    let input = input.trim();
    if is_id(input) {
        return Some(input.into());
    }
    let u = url::Url::parse(input).ok()?;
    u.path_segments()?.find(|seg| is_id(seg)).map(str::to_string)
}

/// Strip the CRX header and return the zip payload.
pub fn crx_zip(bytes: &[u8]) -> Result<&[u8], String> {
    let u32_at = |i: usize| -> Result<usize, String> {
        bytes.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or("truncated CRX".to_string())
    };
    if bytes.starts_with(b"PK") {
        return Ok(bytes); // already a zip
    }
    if !bytes.starts_with(b"Cr24") {
        return Err("not a CRX file (the store may not offer this item for download)".into());
    }
    let start = match u32_at(4)? {
        3 => 12 + u32_at(8)?,
        2 => 16 + u32_at(8)? + u32_at(12)?,
        v => return Err(format!("unsupported CRX version {v}")),
    };
    bytes.get(start..).ok_or("truncated CRX".into())
}

/// Download from the Chrome Web Store and unpack into the extensions directory (blocking; run on a thread).
pub fn download(id: &str) -> Result<PathBuf, String> {
    let url = format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&prodversion=140.0&acceptformat=crx2,crx3&x=id%3D{id}%26uc"
    );
    let mut resp = ureq::get(&url).call().map_err(|e| format!("download failed: {e}"))?;
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("download failed: {e}"))?;
    let zip = crx_zip(&bytes)?;
    let target = dir().join(id);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| format!("could not replace old copy: {e}"))?;
    }
    zip::ZipArchive::new(Cursor::new(zip))
        .and_then(|mut a| a.extract(&target))
        .map_err(|e| format!("could not unpack: {e}"))?;
    // Chromium refuses unpacked extensions containing reserved "_" entries other than _locales.
    let _ = std::fs::remove_dir_all(target.join("_metadata"));
    if !target.join("manifest.json").is_file() {
        return Err("download has no manifest.json".into());
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_from_store_urls() {
        let id = "cjpalhdlnbpafiamejdnhcphjbkeiagm";
        assert_eq!(store_id(id).as_deref(), Some(id));
        assert_eq!(
            store_id(&format!("https://chromewebstore.google.com/detail/ublock-origin/{id}?hl=en")).as_deref(),
            Some(id)
        );
        assert_eq!(store_id(&format!("https://chrome.google.com/webstore/detail/x/{id}")).as_deref(), Some(id));
        assert_eq!(store_id("https://example.com/foo"), None);
        assert_eq!(store_id("not an id"), None);
    }

    #[test]
    fn crx3_header_is_stripped() {
        let mut b = b"Cr24".to_vec();
        b.extend(3u32.to_le_bytes());
        b.extend(5u32.to_le_bytes());
        b.extend([0u8; 5]);
        b.extend(b"PK\x03\x04rest");
        assert_eq!(crx_zip(&b).unwrap(), b"PK\x03\x04rest");
        assert!(crx_zip(b"<html>").is_err());
    }
}

/// The largest icon an extension declares in its manifest, as (bytes, MIME type).
/// `folder_name` is a single folder inside the extensions directory (no path separators).
pub fn icon(folder_name: &str) -> Option<(Vec<u8>, &'static str)> {
    if folder_name.is_empty() || folder_name.contains(['/', '\\']) || folder_name.contains("..") {
        return None;
    }
    let folder = dir().join(folder_name);
    let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(folder.join("manifest.json")).ok()?).ok()?;
    let rel = m["icons"]
        .as_object()?
        .iter()
        .filter_map(|(size, p)| Some((size.parse::<u32>().ok()?, p.as_str()?)))
        .max_by_key(|(size, _)| *size)?
        .1
        .trim_start_matches('/')
        .to_string();
    if rel.contains("..") {
        return None;
    }
    let mime = match rel.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    };
    Some((std::fs::read(folder.join(rel)).ok()?, mime))
}

/// What the extensions UI shows about an installed extension, read from its manifest.
#[derive(Default)]
pub struct Details {
    pub version: String,
    pub description: String,
    /// API permissions plus host access, in plain words where possible.
    pub permissions: Vec<String>,
    /// The Chrome Web Store ID, when the extension was installed from the store.
    pub store_id: Option<String>,
}

pub fn details(folder: &Path) -> Details {
    let Ok(text) = std::fs::read_to_string(folder.join("manifest.json")) else { return Details::default() };
    let Ok(m) = serde_json::from_str::<serde_json::Value>(&text) else { return Details::default() };
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
    };
    let mut perms = strings(&m["permissions"]);
    let hosts: Vec<String> = strings(&m["host_permissions"]).into_iter().chain(perms.iter().filter(|p| p.contains("://") || *p == "<all_urls>").cloned()).collect();
    perms.retain(|p| !p.contains("://") && p != "<all_urls>");
    if hosts.iter().any(|h| h == "<all_urls>" || h.starts_with("*://*/") || h.starts_with("http://*/") || h.starts_with("https://*/")) {
        perms.insert(0, "All websites".into());
    } else if !hosts.is_empty() {
        perms.insert(0, format!("{} site{}", hosts.len(), if hosts.len() == 1 { "" } else { "s" }));
    }
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Details {
        version: m["version"].as_str().unwrap_or_default().to_string(),
        description: localize(folder, &m, m["description"].as_str().unwrap_or_default()),
        permissions: perms,
        store_id: store_id(&name),
    }
}

/// Resolve a `__MSG_key__` placeholder from the extension's default locale.
fn localize(folder: &Path, m: &serde_json::Value, s: &str) -> String {
    let Some(key) = s.strip_prefix("__MSG_").and_then(|k| k.strip_suffix("__")) else { return s.to_string() };
    let locale = m["default_locale"].as_str().unwrap_or("en");
    let path = folder.join("_locales").join(locale).join("messages.json");
    let Ok(text) = std::fs::read_to_string(path) else { return String::new() };
    let Ok(msgs) = serde_json::from_str::<serde_json::Value>(&text) else { return String::new() };
    msgs.as_object()
        .and_then(|o| o.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)))
        .and_then(|(_, v)| v["message"].as_str())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod detail_tests {
    use super::*;

    #[test]
    fn manifest_details() {
        let d = std::env::temp_dir().join(format!("lb-ext-details-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("_locales/en")).unwrap();
        std::fs::write(
            d.join("manifest.json"),
            r#"{"version":"1.2.3","default_locale":"en","description":"__MSG_desc__",
                "permissions":["storage","tabs"],"host_permissions":["<all_urls>"]}"#,
        )
        .unwrap();
        std::fs::write(d.join("_locales/en/messages.json"), r#"{"Desc":{"message":"Blocks ads"}}"#).unwrap();
        let det = details(&d);
        assert_eq!(det.version, "1.2.3");
        assert_eq!(det.description, "Blocks ads");
        assert_eq!(det.permissions, vec!["All websites", "storage", "tabs"]);
        assert_eq!(det.store_id, None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
