//! Direct WebView2 calls that wry doesn't wrap: request blocking, PiP via DevTools, history state,
//! favicons, zoom, find-in-page, and the Downloads folder.

use std::path::PathBuf;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use wry::{WebView, WebViewExtWindows};

use crate::adblock;

/// Toolbar/shortcut entry point into the injected PiP helper (scripts/pip.js).
const PIP_JS: &str = "(async () => window.__lbTogglePip ? await window.__lbTogglePip() : 'unavailable')()";

/// Sites where ad blocking is off, shared with every tab's request filter.
pub type AdblockOff = std::sync::Arc<std::sync::RwLock<std::collections::HashSet<String>>>;

/// Drop ad/tracker requests at the network layer, unless the page's own site (`page_host`, kept
/// up to date by the caller) is in `off`.
pub fn install_blocker(wv: &WebView, page_host: std::sync::Arc<std::sync::Mutex<String>>, off: AdblockOff) {
    let core = wv.webview();
    let env = wv.environment();
    unsafe {
        let _ = core.AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            args.Request()?.Uri(&mut uri)?;
            let allowed = {
                let host = page_host.lock().map(|h| h.clone()).unwrap_or_default();
                !host.is_empty() && off.read().is_ok_and(|s| s.contains(&host))
            };
            if !allowed && adblock::is_blocked(&take_pwstr(uri)) {
                let resp = env.CreateWebResourceResponse(None, 403, &HSTRING::from("Blocked"), &HSTRING::from(""))?;
                args.SetResponse(&resp)?;
            }
            Ok(())
        }));
        let _ = core.add_WebResourceRequested(&handler, &mut 0);
    }
}

/// Toggle picture-in-picture. Runs through the DevTools protocol with `userGesture: true`,
/// because browsers only allow PiP requests that follow a user action.
pub fn toggle_pip(wv: &WebView) {
    let params = serde_json::json!({ "expression": PIP_JS, "awaitPromise": true, "userGesture": true }).to_string();
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())));
    unsafe {
        let _ = wv
            .webview()
            .CallDevToolsProtocolMethod(&HSTRING::from("Runtime.evaluate"), &HSTRING::from(params), &handler);
    }
}

/// Open the browser's print dialog for the page.
pub fn print(wv: &WebView) {
    if let Ok(c) = wv.webview().cast::<ICoreWebView2_16>() {
        unsafe {
            let _ = c.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_BROWSER);
        }
    }
}

pub fn go_back(wv: &WebView) {
    unsafe {
        let _ = wv.webview().GoBack();
    }
}

pub fn go_forward(wv: &WebView) {
    unsafe {
        let _ = wv.webview().GoForward();
    }
}

/// Report (can_go_back, can_go_forward) whenever the tab's history changes.
pub fn on_history(wv: &WebView, notify: impl Fn(bool, bool) + 'static) {
    let core = wv.webview();
    let handler = HistoryChangedEventHandler::create(Box::new(move |sender, _| {
        if let Some(c) = sender {
            let (mut b, mut f) = (BOOL(0), BOOL(0));
            unsafe {
                let _ = c.CanGoBack(&mut b);
                let _ = c.CanGoForward(&mut f);
            }
            notify(b.as_bool(), f.as_bool());
        }
        Ok(())
    }));
    unsafe {
        let _ = core.add_HistoryChanged(&handler, &mut 0);
    }
}

/// Report (playing audio, muted) whenever either changes.
pub fn on_audio(wv: &WebView, notify: impl Fn(bool, bool) + Clone + 'static) {
    let Ok(core) = wv.webview().cast::<ICoreWebView2_8>() else { return };
    let report = |sender: Option<ICoreWebView2>, n: &dyn Fn(bool, bool)| {
        if let Some(c) = sender.and_then(|s| s.cast::<ICoreWebView2_8>().ok()) {
            let (mut playing, mut muted) = (BOOL(0), BOOL(0));
            unsafe {
                let _ = c.IsDocumentPlayingAudio(&mut playing);
                let _ = c.IsMuted(&mut muted);
            }
            n(playing.as_bool(), muted.as_bool());
        }
    };
    let (n1, n2) = (notify.clone(), notify);
    let h1 = IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |s, _| {
        report(s, &n1);
        Ok(())
    }));
    let h2 = IsMutedChangedEventHandler::create(Box::new(move |s, _| {
        report(s, &n2);
        Ok(())
    }));
    unsafe {
        let _ = core.add_IsDocumentPlayingAudioChanged(&h1, &mut 0);
        let _ = core.add_IsMutedChanged(&h2, &mut 0);
    }
}

pub fn set_muted(wv: &WebView, muted: bool) {
    if let Ok(c) = wv.webview().cast::<ICoreWebView2_8>() {
        unsafe {
            let _ = c.SetIsMuted(muted);
        }
    }
}

/// Report the page's favicon as PNG bytes (empty when the page has none).
pub fn on_favicon(wv: &WebView, notify: impl Fn(Vec<u8>) + Clone + 'static) {
    let Ok(core) = wv.webview().cast::<ICoreWebView2_15>() else { return };
    let handler = FaviconChangedEventHandler::create(Box::new(move |sender, _| {
        let Some(c) = sender.and_then(|s| s.cast::<ICoreWebView2_15>().ok()) else { return Ok(()) };
        let n = notify.clone();
        let done = GetFaviconCompletedHandler::create(Box::new(move |_, stream| {
            n(stream.map(|s| read_stream(&s)).unwrap_or_default());
            Ok(())
        }));
        unsafe {
            let _ = c.GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG, &done);
        }
        Ok(())
    }));
    unsafe {
        let _ = core.add_FaviconChanged(&handler, &mut 0);
    }
}

fn read_stream(s: &windows::Win32::System::Com::IStream) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let mut read = 0u32;
        let hr = unsafe { s.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut read)) };
        if hr.is_err() || read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
    }
    out
}

pub fn set_zoom(wv: &WebView, factor: f64) {
    unsafe {
        let _ = wv.controller().SetZoomFactor(factor);
    }
}

/// Report the zoom factor whenever it changes (our buttons or the page's Ctrl+wheel / Ctrl+±).
pub fn on_zoom(wv: &WebView, notify: impl Fn(f64) + 'static) {
    let controller = wv.controller();
    let handler = ZoomFactorChangedEventHandler::create(Box::new(move |sender, _| {
        if let Some(c) = sender {
            let mut z = 1.0;
            unsafe {
                let _ = c.ZoomFactor(&mut z);
            }
            notify(z);
        }
        Ok(())
    }));
    unsafe {
        let _ = controller.add_ZoomFactorChanged(&handler, &mut 0);
    }
}

fn finder(wv: &WebView) -> Option<ICoreWebView2Find> {
    unsafe { wv.webview().cast::<ICoreWebView2_28>().ok()?.Find().ok() }
}

/// Report (active match index, match count) while a find session runs.
pub fn on_find(wv: &WebView, notify: impl Fn(i32, i32) + Clone + 'static) {
    let Some(find) = finder(wv) else { return };
    let report = move |f: Option<ICoreWebView2Find>, n: &dyn Fn(i32, i32)| {
        if let Some(f) = f {
            let (mut i, mut c) = (0, 0);
            unsafe {
                let _ = f.ActiveMatchIndex(&mut i);
                let _ = f.MatchCount(&mut c);
            }
            n(i, c);
        }
    };
    let (n1, n2) = (notify.clone(), notify);
    let h1 = FindMatchCountChangedEventHandler::create(Box::new(move |f, _| {
        report(f, &n1);
        Ok(())
    }));
    let h2 = FindActiveMatchIndexChangedEventHandler::create(Box::new(move |f, _| {
        report(f, &n2);
        Ok(())
    }));
    unsafe {
        let _ = find.add_MatchCountChanged(&h1, &mut 0);
        let _ = find.add_ActiveMatchIndexChanged(&h2, &mut 0);
    }
}

/// Start (or restart) a find for `term`, highlighting every match. Returns false if this WebView2
/// runtime has no find API.
pub fn find_start(wv: &WebView, term: &str) -> bool {
    let Some(find) = finder(wv) else { return false };
    let Ok(env) = wv.environment().cast::<ICoreWebView2Environment15>() else { return false };
    unsafe {
        let Ok(opts) = env.CreateFindOptions() else { return false };
        let _ = opts.SetFindTerm(&HSTRING::from(term));
        let _ = opts.SetShouldHighlightAllMatches(true);
        let _ = opts.SetSuppressDefaultFindDialog(true);
        let done = FindStartCompletedHandler::create(Box::new(|_| Ok(())));
        find.Start(&opts, &done).is_ok()
    }
}

pub fn find_step(wv: &WebView, forward: bool) {
    if let Some(f) = finder(wv) {
        unsafe {
            let _ = if forward { f.FindNext() } else { f.FindPrevious() };
        }
    }
}

pub fn find_stop(wv: &WebView) {
    if let Some(f) = finder(wv) {
        unsafe {
            let _ = f.Stop();
        }
    }
}

/// The user's Downloads folder (falls back to %USERPROFILE%\Downloads).
pub fn downloads_dir() -> PathBuf {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
            let path = PathBuf::from(p.to_string().unwrap_or_default());
            CoTaskMemFree(Some(p.0 as _));
            if !path.as_os_str().is_empty() {
                return path;
            }
        }
    }
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("Downloads")
}

/// Clip the page to a rounded rectangle (`w`/`h`/`radius` in physical pixels). Applied to wry's
/// container window, which clips the WebView2 windows inside it.
pub fn round_corners(wv: &WebView, w: i32, h: i32, radius: i32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
    let mut hwnd = HWND::default();
    unsafe {
        if wv.controller().ParentWindow(&mut hwnd).is_err() || hwnd.is_invalid() {
            return;
        }
        let region = if radius > 0 { Some(CreateRoundRectRgn(0, 0, w + 1, h + 1, radius * 2, radius * 2)) } else { None };
        // On success the system owns the region.
        SetWindowRgn(hwnd, region, true);
    }
}

/// Raise wry's container window above its siblings (used for the dropdown panel).
pub fn bring_to_front(wv: &WebView) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
    let mut hwnd = HWND::default();
    unsafe {
        if wv.controller().ParentWindow(&mut hwnd).is_ok() && !hwnd.is_invalid() {
            let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Colour Windows' title bar to match the toolbar (Windows 11; ignored elsewhere).
pub fn tint_title_bar(window: &impl wry::raw_window_handle::HasWindowHandle, bg: [u8; 3], text: [u8; 3], dark: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };
    use wry::raw_window_handle::RawWindowHandle;
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
    let hwnd = HWND(h.hwnd.get() as *mut _);
    let colorref = |c: [u8; 3]| u32::from(c[0]) | u32::from(c[1]) << 8 | u32::from(c[2]) << 16;
    let set = |attr, value: u32| unsafe {
        let _ = DwmSetWindowAttribute(hwnd, attr, &value as *const u32 as _, 4);
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as u32);
    set(DWMWA_CAPTION_COLOR, colorref(bg));
    set(DWMWA_BORDER_COLOR, colorref(bg));
    set(DWMWA_TEXT_COLOR, colorref(text));
    // Borderless windows lose the rounded corners; ask for them back (Windows 11).
    set(windows::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE, 2 /* DWMWCP_ROUND */);
}

/// How far (in physical pixels) a maximized borderless window hangs past the screen edges: Windows
/// keeps the invisible resize frame outside the work area, so content there would be cut off.
pub fn maximized_overhang(dpi: u32) -> i32 {
    use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
    use windows::Win32::UI::WindowsAndMessaging::{SM_CXPADDEDBORDER, SM_CYSIZEFRAME};
    unsafe { GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi) }
}
