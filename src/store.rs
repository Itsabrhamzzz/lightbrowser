//! Small JSON files in `%LOCALAPPDATA%\LightBrowser\`: settings, last session, bookmarks.

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const DEFAULT_ENGINE: &str = "https://duckduckgo.com/?q=";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Search URL prefix; the query is appended URL-encoded.
    pub engine: String,
    pub restore_session: bool,
    /// UI accent colour, `#rrggbb`.
    pub accent: String,
    /// Extension IDs shown as buttons in the toolbar.
    pub pinned: Vec<String>,
    pub show_bookmarks_bar: bool,
    /// Sites (host names) where ad blocking is turned off.
    pub adblock_off: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { engine: DEFAULT_ENGINE.into(), restore_session: true, accent: crate::theme::DEFAULT_ACCENT.into(), pinned: vec![], show_bookmarks_bar: true, adblock_off: vec![] }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Session {
    pub tabs: Vec<String>,
    pub active: usize,
    /// How many of the first tabs are pinned.
    pub pinned: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub visits: u32,
    /// Last visit, seconds since the Unix epoch.
    #[serde(default)]
    pub last: u64,
}

/// Oldest entries are dropped past this many.
pub const HISTORY_CAP: usize = 5000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DownloadEntry {
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub url: String,
    /// Finished at, seconds since the Unix epoch.
    pub time: u64,
    pub ok: bool,
}

/// Only the most recent downloads are listed.
pub const DOWNLOADS_CAP: usize = 200;

pub fn add_download(list: &mut Vec<DownloadEntry>, entry: DownloadEntry) {
    list.push(entry);
    if list.len() > DOWNLOADS_CAP {
        let extra = list.len() - DOWNLOADS_CAP;
        list.drain(..extra);
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Count a visit to `url` (adding it if new), keeping the list within `HISTORY_CAP`.
pub fn record_visit(h: &mut Vec<HistoryEntry>, url: &str, now: u64) {
    if let Some(e) = h.iter_mut().find(|e| e.url == url) {
        e.visits = e.visits.saturating_add(1);
        e.last = now;
        return;
    }
    if h.len() >= HISTORY_CAP {
        if let Some(i) = h.iter().enumerate().min_by_key(|(_, e)| e.last).map(|(i, _)| i) {
            h.swap_remove(i);
        }
    }
    h.push(HistoryEntry { url: url.into(), title: String::new(), visits: 1, last: now });
}

pub fn set_history_title(h: &mut [HistoryEntry], url: &str, title: &str) -> bool {
    match h.iter_mut().find(|e| e.url == url) {
        Some(e) if e.title != title => {
            e.title = title.into();
            true
        }
        _ => false,
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join("LightBrowser");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn load_in<T: DeserializeOwned + Default>(dir: &Path, name: &str) -> T {
    std::fs::read_to_string(dir.join(name))
        .ok()
        // Tolerate a UTF-8 byte-order mark (some editors add one when a file is edited by hand).
        .and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

/// Write via a temp file + rename so a crash mid-write never leaves a corrupt file.
pub fn save_in<T: Serialize>(dir: &Path, name: &str, value: &T) {
    let Ok(json) = serde_json::to_string_pretty(value) else { return };
    let tmp = dir.join(format!("{name}.tmp"));
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(name));
    }
}

pub fn settings() -> Settings {
    load_in(&dir(), "settings.json")
}
pub fn save_settings(s: &Settings) {
    save_in(&dir(), "settings.json", s)
}
pub fn session() -> Session {
    load_in(&dir(), "session.json")
}
pub fn save_session(s: &Session) {
    save_in(&dir(), "session.json", s)
}
pub fn bookmarks() -> Vec<Bookmark> {
    load_in(&dir(), "bookmarks.json")
}
pub fn save_bookmarks(b: &[Bookmark]) {
    save_in(&dir(), "bookmarks.json", &b)
}
pub fn downloads() -> Vec<DownloadEntry> {
    load_in(&dir(), "downloads.json")
}
pub fn save_downloads(d: &[DownloadEntry]) {
    save_in(&dir(), "downloads.json", &d)
}
pub fn history() -> Vec<HistoryEntry> {
    load_in(&dir(), "history.json")
}
pub fn save_history(h: &[HistoryEntry]) {
    save_in(&dir(), "history.json", &h)
}

/// `dir/name`, or `dir/stem (1).ext`, `dir/stem (2).ext`… if that already exists.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| name.into());
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|i| dir.join(format!("{stem} ({i}){ext}")))
        .find(|c| !c.exists())
        .expect("unbounded range")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lb-store-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trip_and_defaults() {
        let d = tmp("rt");
        let s: Settings = load_in(&d, "settings.json");
        assert_eq!(s, Settings::default());
        let b = vec![Bookmark { title: "Rust".into(), url: "https://rust-lang.org/".into() }];
        save_in(&d, "bookmarks.json", &b);
        let back: Vec<Bookmark> = load_in(&d, "bookmarks.json");
        assert_eq!(back, b);
        // Missing fields fall back to defaults.
        std::fs::write(d.join("settings.json"), r#"{"engine":"x"}"#).unwrap();
        let s: Settings = load_in(&d, "settings.json");
        assert!(s.restore_session && s.engine == "x");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn downloads_keep_the_newest() {
        let entry = |i: u64| DownloadEntry { name: format!("f{i}"), path: PathBuf::from(format!("f{i}")), url: String::new(), time: i, ok: true };
        let mut list = vec![];
        for i in 0..(DOWNLOADS_CAP as u64 + 5) {
            add_download(&mut list, entry(i));
        }
        assert_eq!(list.len(), DOWNLOADS_CAP);
        assert_eq!(list[0].time, 5);
        assert_eq!(list.last().unwrap().time, DOWNLOADS_CAP as u64 + 4);
    }

    #[test]
    fn session_pins_round_trip() {
        let d = tmp("session");
        // A session saved before pinning existed has no "pinned" field.
        std::fs::write(d.join("session.json"), r#"{"tabs":["https://a.com/"],"active":0}"#).unwrap();
        let s: Session = load_in(&d, "session.json");
        assert_eq!(s.pinned, 0);
        let s = Session { tabs: vec!["https://a.com/".into(), "https://b.com/".into()], active: 1, pinned: 1 };
        save_in(&d, "session.json", &s);
        assert_eq!(load_in::<Session>(&d, "session.json"), s);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn history_upsert_and_cap() {
        let mut h = vec![];
        record_visit(&mut h, "https://a.com/", 10);
        record_visit(&mut h, "https://a.com/", 20);
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].visits, h[0].last), (2, 20));
        assert!(set_history_title(&mut h, "https://a.com/", "A"));
        assert!(!set_history_title(&mut h, "https://a.com/", "A"));
        assert!(!set_history_title(&mut h, "https://nope.com/", "N"));

        let mut h: Vec<HistoryEntry> = (0..HISTORY_CAP as u64)
            .map(|i| HistoryEntry { url: format!("https://x{i}.com/"), title: String::new(), visits: 1, last: i + 100 })
            .collect();
        record_visit(&mut h, "https://new.com/", 99_999);
        assert_eq!(h.len(), HISTORY_CAP);
        assert!(!h.iter().any(|e| e.url == "https://x0.com/")); // the oldest went
        assert!(h.iter().any(|e| e.url == "https://new.com/"));
    }

    #[test]
    fn unique_names() {
        let d = tmp("uniq");
        assert_eq!(unique_path(&d, "a.pdf"), d.join("a.pdf"));
        std::fs::write(d.join("a.pdf"), "").unwrap();
        assert_eq!(unique_path(&d, "a.pdf"), d.join("a (1).pdf"));
        std::fs::write(d.join("a (1).pdf"), "").unwrap();
        assert_eq!(unique_path(&d, "a.pdf"), d.join("a (2).pdf"));
        std::fs::write(d.join("README"), "").unwrap();
        assert_eq!(unique_path(&d, "README"), d.join("README (1)"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
