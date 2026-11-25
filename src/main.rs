#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adblock;
mod chrome;
mod extensions;
mod split;
mod store;
mod suggest;
mod theme;
mod url_input;
mod wv2;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use chrome::{Icon, TabEvent, TabView, BTN, TAB_H};
use theme::Palette;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, Rect as ERect, Stroke, TextureHandle, Vec2};
use wry::WebViewBuilderExtWindows;
use wry::{dpi, NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder};

/// Height of the two toolbar rows; a strip (find bar / download notice) can be added below.
const CHROME_H: f32 = 92.0;
const STRIP_H: f32 = 44.0;
/// Extra toolbar height while the bookmarks bar is shown.
const BM_BAR_H: f32 = 30.0;
const PILL_H: f32 = 34.0;
/// Side padding of the toolbar rows. (The page itself runs to the window's edges.)
const INSET: f32 = 8.0;
/// Matches the rounding Windows 11 gives the window itself.
const PAGE_RADIUS: f32 = 8.0;
const WIN_BTN_W: f32 = 46.0;
const WIN_BTN_H: f32 = 36.0;
/// How close to the window edge the pointer must be to resize (there is no native frame).
const RESIZE_GRIP: f32 = 6.0;
const HOME: &str = "http://lb.localhost/";
const EXT_PAGE: &str = "http://lb.localhost/extensions";
const EXT_MENU: &str = "http://lb.localhost/ext-menu";
const HISTORY_PAGE: &str = "http://lb.localhost/history";
const DOWNLOADS_PAGE: &str = "http://lb.localhost/downloads";
const DOWNLOADS_HTML: &str = include_str!("downloads.html");
const MENU_PAGE: &str = "http://lb.localhost/menu";
const MENU_HTML: &str = include_str!("menu.html");
const BOOKMARK_HTML: &str = include_str!("bookmark.html");
const HISTORY_HTML: &str = include_str!("history.html");
const EXT_MENU_HTML: &str = include_str!("ext-menu.html");
const PANEL_SCRIPT: &str = include_str!("scripts/panel.js");
const START_HTML: &str = include_str!("start.html");
const EXT_HTML: &str = include_str!("extensions.html");
const KEY_SCRIPT: &str = include_str!("scripts/key.js");
const AD_SCRIPT: &str = include_str!("scripts/ad.js");
const STORE_SCRIPT: &str = include_str!("scripts/store.js");
const PIP_SCRIPT: &str = include_str!("scripts/pip.js");
const ICON_PNG: &[u8] = include_bytes!("../assets/icon-256.png");

const ZOOM_STEPS: [f64; 17] = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Action {
    New,
    Close,
    Focus,
    Reload,
    Back,
    Forward,
    Fullscreen(bool),
    Pip,
    Extensions,
    History,
    Downloads,
    BookmarksBar,
    /// Ctrl+Shift+S: split the current tab with a new tab, or end its split.
    Split,
    DevTools,
    Print,
    Find,
    Esc,
    Bookmark,
    NextTab,
    PrevTab,
    /// Ctrl+1…9 (9 = last tab).
    GoTab(usize),
    Reopen,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// The user clicked inside the page.
    PageDown,
}

impl Action {
    fn from_ipc(s: &str) -> Option<Self> {
        Some(match s {
            "new" => Self::New,
            "close" => Self::Close,
            "focus" => Self::Focus,
            "reload" => Self::Reload,
            "back" => Self::Back,
            "forward" => Self::Forward,
            "pip" => Self::Pip,
            "find" => Self::Find,
            "history" => Self::History,
            "downloads" => Self::Downloads,
            "devtools" => Self::DevTools,
            "bmbar" => Self::BookmarksBar,
            "split" => Self::Split,
            "esc" => Self::Esc,
            "bookmark" => Self::Bookmark,
            "next" => Self::NextTab,
            "prev" => Self::PrevTab,
            "reopen" => Self::Reopen,
            "pagedown" => Self::PageDown,
            _ => Self::GoTab(s.strip_prefix("tab:")?.parse().ok()?),
        })
    }
}

enum Ev {
    Title(u64, String),
    Url(u64, String),
    Loading(u64, bool),
    /// A page finished loading, at this URL (after any redirects).
    Loaded(u64, String),
    History(u64, bool, bool),
    Favicon(u64, Vec<u8>),
    Zoom(u64, f64),
    /// (playing audio, muted)
    Audio(u64, bool, bool),
    /// The tab's page got keyboard focus.
    Focus(u64),
    /// The tab's page entered / left fullscreen (WebView2's own event).
    Fullscreen(u64, bool),
    FindCount(u64, i32, i32),
    NewTab(String),
    Act(Action),
    /// A download finished: (source URL, saved file, succeeded).
    Downloaded(String, Option<PathBuf>, bool),
    Ext(extensions::ExtEvent),
    /// A command from the extensions page (or the Web Store install button).
    ExtCmd(String),
    /// A settings/bookmark command from the start page.
    LocalCmd(String),
    PanelClose,
    PanelSize(f32, f32),
    /// A click (or `ready`) from the shared menu page in the dropdown panel.
    MenuCmd(String),
}

struct Tab {
    id: u64,
    webview: WebView,
    title: String,
    url: String,
    loading: bool,
    can_back: bool,
    can_fwd: bool,
    favicon: Option<TextureHandle>,
    zoom: f64,
    audible: bool,
    muted: bool,
    /// Pinned tabs are icon-only and always come first.
    pinned: bool,
}

/// A tab waiting to be opened (next frame, when the window frame is available).
struct NewTab {
    url: String,
    /// Insert at this index (default: the end).
    at: Option<usize>,
    /// Switch to it; `false` opens it in the background (a page opened it while a split shows).
    switch: bool,
}

impl NewTab {
    fn new(url: impl Into<String>) -> Self {
        Self { url: url.into(), at: None, switch: true }
    }
}

struct FindBar {
    text: String,
    /// (active match index, match count) as reported by WebView2.
    counts: (i32, i32),
    focus: bool,
}

#[derive(Clone, PartialEq, Debug)]
enum PanelKind {
    Menu,
    /// An extension's popup (extension ID).
    Popup(String),
    /// A context menu on the shared menu page; the tag says what it's for (e.g. `tab:3`).
    Context(String),
    /// Address-bar suggestions (shared menu page; keyboard focus stays in the address bar).
    Suggest,
    /// The "Add / Edit bookmark" dialog under the star (its page URL carries the details).
    Bookmark(String),
}

impl PanelKind {
    fn uses_menu_page(&self) -> bool {
        matches!(self, Self::Context(_) | Self::Suggest)
    }
}

/// Where the dropdown panel sits horizontally.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Anchor {
    /// Right edge at this x (the button that opened it).
    Right(f32),
    /// Left edge at this x (a right-click).
    Left(f32),
    /// Exactly this span: (left, width) — the address pill.
    Span(f32, f32),
}

/// The dropdown panel under the toolbar (a second, borderless webview).
struct OpenPanel {
    kind: PanelKind,
    anchor: Anchor,
    size: Vec2,
}

struct Toast {
    text: String,
    path: Option<PathBuf>,
    until: f64,
}

struct BrowserApp {
    /// Right edge of the bookmark star, to anchor the bookmark dialog under it.
    star_right: f32,
    /// A window torn off from another: it doesn't restore or save the session.
    secondary: bool,
    /// Set when this window should close (a torn-off window's last tab was closed).
    close_window: bool,
    web_context: WebContext,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    address: String,
    shown_url: String,
    addr_focused: bool,
    address_id: egui::Id,
    find_id: egui::Id,
    /// What the webviews were last laid out for: window size, content top, and the split pairs.
    last_layout: Option<(f32, f32, f32, Vec<(u64, u64, i32)>)>,
    /// Split view pairs (see `split.rs`).
    splits: Vec<split::Split>,
    /// When the next opened tab arrives, pair it to the right of this tab.
    split_next: Option<u64>,
    /// The tab whose page is fullscreen (in a split, only that page is shown).
    fullscreen_tab: Option<u64>,
    /// Leave fullscreen next frame (its page was closed).
    exit_fullscreen: bool,
    fullscreen: bool,
    saved_window: Option<(egui::Pos2, egui::Vec2)>,
    was_maximized: bool,
    /// Padding (points) that keeps content on screen while maximized; 0 otherwise.
    frame_pad: f32,
    tx: Sender<Ev>,
    rx: Receiver<Ev>,
    pending: Vec<NewTab>,
    /// After restoring a session, which tab to show once all have opened.
    restore_active: Option<usize>,
    scroll_to_active: bool,
    closed: Vec<String>,
    settings: store::Settings,
    bookmarks: Vec<store::Bookmark>,
    saved_session: store::Session,
    title_sent: String,
    find: Option<FindBar>,
    toast: Option<Toast>,
    installed_accent: String,
    panel: Option<WebView>,
    panel_open: Option<OpenPanel>,
    panel_req: Option<(PanelKind, Anchor)>,
    /// The page the panel webview has loaded (so the menu page isn't reloaded for every update).
    panel_loaded: String,
    /// JSON for the menu page (`window.__lbMenu`), re-sent when the page reports `ready`.
    panel_data: Option<String>,
    /// Visited pages, shared with the `lb.localhost` request handler for the history page.
    history: Arc<Mutex<Vec<store::HistoryEntry>>>,
    history_dirty: bool,
    history_saved_at: f64,
    sugs: Vec<suggest::Suggestion>,
    sug_sel: usize,
    /// Close the suggestions this long after the address bar lost focus (unless one was clicked).
    sug_close_at: Option<f64>,
    /// Take keyboard focus away from the address bar next frame (the page or a navigation took over).
    blur_address: bool,
    /// Bookmark being renamed inline: (index, text, needs focus).
    bm_rename: Option<(usize, String, bool)>,
    /// key.js with this run's IPC secret filled in.
    key_script: String,
    ipc_token: Arc<str>,
    /// Sites with ad blocking turned off, shared with every tab's request filter.
    adblock_off: wv2::AdblockOff,
    /// Site icons by host, collected from tabs, for the bookmarks bar.
    favicons: std::collections::HashMap<String, TextureHandle>,
    /// Set when the dropdown panel was used this frame (so a chrome click doesn't also close it).
    panel_touched: bool,
    puzzle_right: f32,
    pin_anchors: std::collections::HashMap<String, f32>,
    ext_icons: std::collections::HashMap<String, Option<TextureHandle>>,
    tinted: Option<Palette>,
    ext_loaded: bool,
    ext_folders: std::collections::HashMap<String, PathBuf>,
    ext_items: Vec<(String, String, bool)>,
    ext_status: (String, bool),
}

/// A random secret for this run of the browser (128 bits from the OS-seeded hasher keys).
fn new_ipc_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// A shortcut action from a page, accepted only with this run's secret (`k:<token>:<action>`).
/// Pages can call the IPC bridge directly, but only our injected key.js knows the token.
fn shortcut_from_ipc(body: &str, token: &str) -> Option<Action> {
    let rest = body.strip_prefix("k:")?.strip_prefix(token)?.strip_prefix(':')?;
    Action::from_ipc(rest)
}

fn tab_mut(tabs: &mut [Tab], id: u64) -> Option<&mut Tab> {
    tabs.iter_mut().find(|t| t.id == id)
}

/// Short form shown while the address bar isn't being edited.
fn display_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {
            let host = u.host_str().unwrap_or("");
            if host == "lb.localhost" {
                return String::new();
            }
            let host = host.strip_prefix("www.").unwrap_or(host);
            let path = u.path().trim_end_matches('/');
            format!("{host}{path}")
        }
        _ => url.to_string(),
    }
}

/// Host name without a leading `www.` ("" if there is none).
fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_ascii_lowercase()))
        .unwrap_or_default()
}

fn is_internal(url: &str) -> bool {
    url.starts_with("http://lb.localhost") || url.starts_with("chrome-extension://") || url == "about:blank"
}

fn json_response(body: String) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .body(std::borrow::Cow::Owned(body.into_bytes()))
        .expect("static response")
}

/// Lock a mutex even if a panicking thread poisoned it (the data is plain values).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

type History = Arc<Mutex<Vec<store::HistoryEntry>>>;

/// Pages served on http://lb.localhost/: the start page, extensions page, and a tiny JSON API.
fn serve_local(path: &str, history: &History) -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    match path {
        "/api/downloads" => {
            // Newest first; `i` is the index the page sends back in commands.
            let list: Vec<_> = store::downloads()
                .into_iter()
                .enumerate()
                .rev()
                .map(|(i, d)| {
                    let exists = d.path.exists();
                    serde_json::json!({ "i": i, "name": d.name, "path": d.path, "url": d.url, "time": d.time, "ok": d.ok, "exists": exists })
                })
                .collect();
            json_response(serde_json::Value::Array(list).to_string())
        }
        "/api/history" => {
            let mut h = lock(history).clone();
            h.sort_by(|a, b| b.last.cmp(&a.last));
            json_response(serde_json::to_string(&h).unwrap_or_default())
        }
        p if p.starts_with("/ext-icon/") => match extensions::icon(&p["/ext-icon/".len()..]) {
            Some((bytes, mime)) => wry::http::Response::builder()
                .header("Content-Type", mime)
                .body(std::borrow::Cow::Owned(bytes))
                .expect("static response"),
            None => wry::http::Response::builder().status(404).body(std::borrow::Cow::Borrowed(&[][..])).expect("static response"),
        },
        "/favicon.png" => wry::http::Response::builder()
            .header("Content-Type", "image/png")
            .body(std::borrow::Cow::Borrowed(ICON_PNG))
            .expect("static response"),
        "/theme.css" => wry::http::Response::builder()
            .header("Content-Type", "text/css; charset=utf-8")
            .header("Cache-Control", "no-store")
            .body(std::borrow::Cow::Owned(theme::css(&store::settings().accent).into_bytes()))
            .expect("static response"),
        "/api/settings" => json_response(serde_json::to_string(&store::settings()).unwrap_or_default()),
        "/api/bookmarks" => json_response(serde_json::to_string(&store::bookmarks()).unwrap_or_default()),
        _ => {
            let page: &'static str = match path {
                "/extensions" => EXT_HTML,
                "/ext-menu" => EXT_MENU_HTML,
                "/history" => HISTORY_HTML,
                "/downloads" => DOWNLOADS_HTML,
                "/menu" => MENU_HTML,
                "/bookmark" => BOOKMARK_HTML,
                _ => START_HTML,
            };
            wry::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .body(std::borrow::Cow::Borrowed(page.as_bytes()))
                .expect("static response")
        }
    }
}

impl BrowserApp {
    fn new(launch: Launch) -> Self {
        let (tx, rx) = channel();
        let settings = store::settings();
        let session = store::session();
        let secondary = launch.url.is_some();
        let ipc_token = new_ipc_token();
        let adblock_off = Arc::new(std::sync::RwLock::new(settings.adblock_off.iter().cloned().collect()));
        let (pending, restore_active) = if let Some(url) = &launch.url {
            (vec![NewTab::new(url)], None)
        } else if settings.restore_session && !session.tabs.is_empty() {
            (session.tabs.iter().map(NewTab::new).collect(), Some(session.active))
        } else {
            (vec![NewTab::new(HOME)], None)
        };
        Self {
            star_right: 0.0,
            secondary,
            close_window: false,
            web_context: WebContext::new(Some(store::dir())),
            tabs: vec![],
            active: 0,
            next_id: 1,
            address: String::new(),
            shown_url: String::new(),
            addr_focused: false,
            address_id: egui::Id::unique("address"),
            find_id: egui::Id::unique("find"),
            last_layout: None,
            splits: vec![],
            split_next: None,
            fullscreen_tab: None,
            exit_fullscreen: false,
            fullscreen: false,
            saved_window: None,
            was_maximized: false,
            frame_pad: 0.0,
            tx,
            rx,
            pending,
            restore_active,
            scroll_to_active: true,
            closed: vec![],
            settings,
            bookmarks: store::bookmarks(),
            saved_session: session,
            title_sent: String::new(),
            find: None,
            toast: None,
            installed_accent: String::new(),
            panel: None,
            panel_open: None,
            panel_req: None,
            panel_loaded: String::new(),
            panel_data: None,
            history: Arc::new(Mutex::new(store::history())),
            history_dirty: false,
            history_saved_at: 0.0,
            sugs: vec![],
            sug_sel: 0,
            sug_close_at: None,
            blur_address: false,
            bm_rename: None,
            favicons: Default::default(),
            adblock_off,
            key_script: KEY_SCRIPT.replace("__LB_TOKEN__", &ipc_token),
            ipc_token: ipc_token.into(),
            panel_touched: false,
            puzzle_right: 0.0,
            pin_anchors: Default::default(),
            ext_icons: Default::default(),
            tinted: None,
            ext_loaded: false,
            ext_folders: Default::default(),
            ext_items: vec![],
            ext_status: (String::new(), false),
        }
    }

    /// A cloneable, thread-safe way for callbacks to post events and wake the UI.
    fn sender(&self, ctx: &egui::Context) -> impl Fn(Ev) + Clone + Send + 'static {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        move |e| {
            let _ = tx.send(e);
            ctx.request_repaint();
        }
    }

    fn content_top(&self) -> f32 {
        if self.fullscreen {
            0.0
        } else if self.find.is_some() || self.toast.is_some() {
            self.frame_pad + self.chrome_h() + STRIP_H
        } else {
            self.frame_pad + self.chrome_h()
        }
    }

    /// Where the page card sits (window coordinates, points).
    fn page_rect(&self, w: f32, h: f32) -> ERect {
        let top = self.content_top();
        let inset = if self.fullscreen { 0.0 } else { self.frame_pad };
        ERect::from_min_max(egui::pos2(inset, top), egui::pos2((w - inset).max(inset + 1.0), (h - inset).max(top + 1.0)))
    }

    fn bounds(page: ERect) -> Rect {
        Rect {
            position: dpi::LogicalPosition::new(page.left() as f64, page.top() as f64).into(),
            size: dpi::LogicalSize::new(page.width() as f64, page.height() as f64).into(),
        }
    }

    fn open_tab(&mut self, frame: &eframe::Frame, ctx: &egui::Context, req: NewTab) {
        let url = req.url.as_str();
        let id = self.next_id;
        self.next_id += 1;
        let r = ctx.content_rect();
        let bounds = Self::bounds(self.page_rect(r.width(), r.height()));
        let send = self.sender(ctx);
        let (s_nav, s_title, s_win, s_load, s_ipc, s_dl) =
            (send.clone(), send.clone(), send.clone(), send.clone(), send.clone(), send.clone());
        let history = self.history.clone();
        let page_host = Arc::new(Mutex::new(host_of(url)));
        let nav_host = page_host.clone();
        let token = self.ipc_token.clone();
        let built = WebViewBuilder::new_with_web_context(&mut self.web_context)
            .with_url(url)
            .with_bounds(bounds)
            .with_initialization_script(&self.key_script)
            .with_initialization_script(AD_SCRIPT)
            .with_initialization_script_for_main_only(PIP_SCRIPT, false)
            .with_initialization_script(STORE_SCRIPT)
            // A background tab mustn't take keyboard focus from the page being used.
            .with_focused(req.switch)
            .with_devtools(true)
            .with_browser_extensions_enabled(true)
            .with_custom_protocol("lb".into(), move |_id, req| serve_local(req.uri().path(), &history))
            .with_navigation_handler(move |u| {
                *lock(&nav_host) = host_of(&u);
                s_nav(Ev::Url(id, u));
                true
            })
            .with_document_title_changed_handler(move |t| s_title(Ev::Title(id, t)))
            .with_new_window_req_handler(move |u, _| {
                s_win(Ev::NewTab(u));
                NewWindowResponse::Deny
            })
            .with_on_page_load_handler(move |ev, url| match ev {
                PageLoadEvent::Started => s_load(Ev::Loading(id, true)),
                PageLoadEvent::Finished => s_load(Ev::Loaded(id, url)),
            })
            .with_download_started_handler(|_url, path| {
                // Save into Downloads without overwriting an existing file.
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "download".into());
                let dir = wv2::downloads_dir();
                let _ = std::fs::create_dir_all(&dir);
                *path = store::unique_path(&dir, &name);
                true
            })
            .with_download_completed_handler(move |url, path, ok| s_dl(Ev::Downloaded(url, path, ok)))
            .with_ipc_handler(move |req| {
                let body = req.body();
                let host = req.uri().host().unwrap_or("");
                if let Some(cmd) = body.strip_prefix("ext:") {
                    // Only our own pages may manage extensions; the Web Store may only request installs.
                    if host == "lb.localhost" || (host == "chromewebstore.google.com" && cmd.starts_with("install:")) {
                        s_ipc(Ev::ExtCmd(cmd.to_string()));
                    }
                } else if ["set:", "bm:", "hist:", "dl:"].iter().any(|p| body.starts_with(p)) {
                    if host == "lb.localhost" {
                        s_ipc(Ev::LocalCmd(body.clone()));
                    }
                } else if let Some(a) = shortcut_from_ipc(body, &token) {
                    if a == Action::PageDown {
                        // A click in this page also makes it the active side of a split (the focus
                        // event covers clicks inside iframes, which don't send this).
                        s_ipc(Ev::Focus(id));
                    }
                    s_ipc(Ev::Act(a));
                }
            })
            .build_as_child(frame);

        let webview = match built {
            Ok(w) => w,
            Err(e) => return eprintln!("failed to create webview: {e}"),
        };
        wv2::install_blocker(&webview, page_host, self.adblock_off.clone());
        if !self.fullscreen {
            let page = self.page_rect(r.width(), r.height());
            let ppp = ctx.pixels_per_point();
            wv2::round_corners(
                &webview,
                (page.width() * ppp).round() as i32,
                (page.height() * ppp).round() as i32,
                (PAGE_RADIUS * ppp).round() as i32,
            );
        }
        let s = send.clone();
        wv2::on_history(&webview, move |b, f| s(Ev::History(id, b, f)));
        let s = send.clone();
        wv2::on_favicon(&webview, move |png| s(Ev::Favicon(id, png)));
        let s = send.clone();
        wv2::on_zoom(&webview, move |z| s(Ev::Zoom(id, z)));
        let s = send.clone();
        wv2::on_find(&webview, move |i, n| s(Ev::FindCount(id, i, n)));
        let s = send.clone();
        wv2::on_audio(&webview, move |a, m| s(Ev::Audio(id, a, m)));
        let s = send.clone();
        wv2::on_focus(&webview, move || s(Ev::Focus(id)));
        let s = send.clone();
        wv2::on_fullscreen(&webview, move |on| s(Ev::Fullscreen(id, on)));
        if !self.ext_loaded {
            self.ext_loaded = true;
            extensions::load_all(&webview, self.ext_notify(ctx));
        }
        if !req.switch {
            let _ = webview.set_visible(false);
        }
        let at = req.at.unwrap_or(self.tabs.len()).min(self.tabs.len());
        self.tabs.insert(at, Tab {
            id,
            webview,
            title: String::new(),
            url: url.into(),
            loading: true,
            can_back: false,
            can_fwd: false,
            favicon: None,
            zoom: 1.0,
            audible: false,
            muted: false,
            pinned: false,
        });
        if !req.switch {
            // Opened in the background: the same tab stays active (it may have shifted right).
            if at <= self.active && self.tabs.len() > 1 {
                self.active += 1;
            }
            return;
        }
        self.active = at;
        if let Some(left) = self.split_next.take() {
            if let Some(li) = self.tabs.iter().position(|t| t.id == left) {
                self.reposition(self.active, li + 1);
                self.splits.push(split::Split { left, right: id, ratio: split::DEFAULT_RATIO });
            }
        }
        self.show_active();
        self.after_switch();
    }

    fn after_switch(&mut self) {
        self.close_panel();
        self.shown_url.clear();
        self.scroll_to_active = true;
        if let Some(f) = self.find.take() {
            let _ = f; // find results belong to the previous tab
            for t in &self.tabs {
                wv2::find_stop(&t.webview);
            }
        }
    }

    fn select(&mut self, i: usize) {
        if i >= self.tabs.len() || i == self.active {
            return;
        }
        // The other side of the pair that's showing: just make it the active side.
        let (id, cur) = (self.tabs[i].id, self.tabs[self.active].id);
        if split::partner(&self.splits, cur) == Some(id) {
            self.focus_side(id);
            let _ = self.tabs[i].webview.focus();
            return;
        }
        self.active = i;
        self.show_active();
        self.after_switch();
    }

    /// Queue a tab the user asked for: it opens right after the split on screen (if any) and is
    /// switched to.
    fn queue_tab(&mut self, url: impl Into<String>) {
        let at = self.shown_split().map(|_| self.split_insert_index());
        self.pending.push(NewTab { url: url.into(), at, switch: true });
    }

    /// Queue a tab a page opened: while a split shows, it opens in the background after the split
    /// so the split stays on screen; otherwise like `queue_tab`.
    fn queue_from_page(&mut self, url: String) {
        let shown = self.shown_split().is_some();
        let at = shown.then(|| self.split_insert_index());
        self.pending.push(NewTab { url, at, switch: !shown });
    }

    fn split_insert_index(&self) -> usize {
        let ids: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        split::insert_index(&ids, &self.splits, self.active)
    }

    /// Show the active tab — and its split partner, if it has one — and hide every other tab.
    fn show_active(&self) {
        let ids: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        let fullscreen = self.fullscreen_tab.filter(|_| self.fullscreen);
        let shown = split::visible(&ids, &self.splits, self.active, fullscreen);
        for t in &self.tabs {
            let _ = t.webview.set_visible(shown.contains(&t.id));
        }
    }

    // ---- Split view --------------------------------------------------------------------------

    /// Shadows for both panes, the active side's outline, and the draggable divider between them.
    fn draw_split_chrome(&mut self, ui: &mut egui::Ui, pal: &Palette, page: ERect, s: split::Split) {
        let (l, r) = split::pane_rects(page, s.ratio);
        let radius = CornerRadius::same(PAGE_RADIUS as u8);
        let p = ui.painter().clone();
        p.add(pal.card_shadow().as_shape(l, radius));
        p.add(pal.card_shadow().as_shape(r, radius));
        let active = if self.tabs.get(self.active).is_some_and(|t| t.id == s.left) { l } else { r };
        p.rect_stroke(
            active.expand(2.0),
            CornerRadius::same(PAGE_RADIUS as u8 + 2),
            Stroke::new(2.0, pal.accent),
            egui::StrokeKind::Inside,
        );

        let gap = ERect::from_min_max(egui::pos2(l.right(), l.top()), egui::pos2(r.left(), l.bottom()));
        let resp = ui.interact(gap.expand2(Vec2::new(3.0, 0.0)), egui::Id::unique("split-divider"), egui::Sense::click_and_drag());
        let hot = resp.hovered() || resp.dragged();
        if hot {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        let grip = ERect::from_center_size(gap.center(), Vec2::new(4.0, 36.0));
        p.rect_filled(grip, CornerRadius::same(2), if hot { pal.accent } else { pal.border });
        let new_ratio = if resp.double_clicked() {
            Some(split::DEFAULT_RATIO)
        } else if resp.dragged() {
            resp.interact_pointer_pos()
                .map(|pos| split::clamp_ratio((pos.x - page.left() - split::GAP / 2.0) / (page.width() - split::GAP), page.width()))
        } else {
            None
        };
        if let (Some(ratio), Some(k)) = (new_ratio, split::find(&self.splits, s.left)) {
            self.splits[k].ratio = ratio;
        }
    }

    /// The pair the active tab belongs to, if any.
    fn shown_split(&self) -> Option<split::Split> {
        let id = self.tabs.get(self.active)?.id;
        split::find(&self.splits, id).map(|k| self.splits[k])
    }

    /// Where tab `id`'s page goes inside the page area: its pane if it's in a pair, else all of it.
    fn pane_of(&self, id: u64, page: ERect) -> ERect {
        match split::find(&self.splits, id).map(|k| self.splits[k]) {
            Some(s) if !self.fullscreen => {
                let (l, r) = split::pane_rects(page, s.ratio);
                if s.left == id { l } else { r }
            }
            // Unpaired, or fullscreen (the fullscreen page fills the window).
            _ => page,
        }
    }

    /// Pair the active tab (left) with tab `with` (moved next to it), or with a new tab.
    fn start_split(&mut self, with: Option<usize>) {
        let Some(cur) = self.tabs.get(self.active) else { return };
        if cur.pinned || split::find(&self.splits, cur.id).is_some() {
            return;
        }
        let cur_id = cur.id;
        match with {
            Some(i) => {
                let Some(t) = self.tabs.get(i) else { return };
                if i == self.active || t.pinned || split::find(&self.splits, t.id).is_some() {
                    return;
                }
                let right = t.id;
                // After removing `i`, the slot right after the current tab is `active + 1` if `i` was
                // to its right, else `active` (the current tab shifted left by one).
                let to = if i > self.active { self.active + 1 } else { self.active };
                self.reposition(i, to);
                self.splits.push(split::Split { left: cur_id, right, ratio: split::DEFAULT_RATIO });
                self.show_active();
            }
            None => {
                self.split_next = Some(cur_id);
                self.queue_tab(HOME);
            }
        }
    }

    /// Dissolve the pair containing `tab`; both tabs stay, the active one goes full width.
    fn end_split(&mut self, tab: u64) {
        if let Some(k) = split::find(&self.splits, tab) {
            self.splits.remove(k);
            self.show_active();
        }
    }

    /// Swap a pair's two tabs (and keep each page's width).
    fn swap_split(&mut self, tab: u64) {
        let Some(k) = split::find(&self.splits, tab) else { return };
        let s = self.splits[k];
        let Some(li) = self.tabs.iter().position(|t| t.id == s.left) else { return };
        let active_id = self.tabs[self.active].id;
        self.tabs.swap(li, li + 1);
        self.active = self.tabs.iter().position(|t| t.id == active_id).unwrap_or(self.active);
        self.splits[k] = split::Split { left: s.right, right: s.left, ratio: 1.0 - s.ratio };
    }

    /// Make `tab` the active side of the pair on screen (the toolbar then acts on it).
    fn focus_side(&mut self, tab: u64) {
        let Some(s) = self.shown_split() else { return };
        if (s.left != tab && s.right != tab) || self.tabs[self.active].id == tab {
            return;
        }
        if let Some(i) = self.tabs.iter().position(|t| t.id == tab) {
            self.active = i;
            self.shown_url.clear();
            // Typed-but-unsent address text belonged to the other side: drop it.
            self.blur_address = true;
            if self.panel_is(&PanelKind::Suggest) {
                self.close_panel();
            }
            if self.find.take().is_some() {
                for t in &self.tabs {
                    wv2::find_stop(&t.webview);
                }
            }
        }
    }

    fn close(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        // Decide what to show next before the split is dissolved (the partner, for a split side).
        let ids: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        let next_active = split::active_after_close(&ids, &self.splits, self.active, i);
        if self.fullscreen_tab == Some(self.tabs[i].id) {
            // Its "fullscreen ended" event will never come; leave fullscreen ourselves.
            self.fullscreen_tab = None;
            self.exit_fullscreen = true;
        }
        self.end_split(self.tabs[i].id);
        let url = self.tabs[i].url.clone();
        if !url.starts_with("http://lb.localhost") && url != "about:blank" {
            self.closed.push(url);
            if self.closed.len() > 20 {
                self.closed.remove(0);
            }
        }
        if self.tabs.len() == 1 {
            // Closing the last tab closes the window (torn-off windows are separate processes,
            // so they stay open). The session was already saved with this tab in it.
            self.close_window = true;
            return;
        }
        let was_active = i == self.active;
        self.tabs.remove(i);
        self.active = next_active.min(self.tabs.len() - 1);
        self.show_active();
        if was_active {
            let _ = self.tabs[self.active].webview.focus();
        }
        self.after_switch();
    }

    /// Open tab `i` in a new LightBrowser window placed where it was dropped (`at`, window
    /// coordinates), and remove it here. The only tab in a window can't be torn off.
    fn detach(&mut self, ctx: &egui::Context, i: usize, at: egui::Pos2) {
        if self.tabs.len() <= 1 || i >= self.tabs.len() {
            return;
        }
        let url = self.tabs[i].url.clone();
        let (outer, inner) = ctx.input(|inp| (inp.viewport().outer_rect, inp.viewport().inner_rect));
        let origin = inner.or(outer).map_or(egui::Pos2::ZERO, |r| r.min);
        let size = inner.map_or(Vec2::new(1100.0, 750.0), |r| r.size());
        // Put the new window so its first tab sits under the pointer.
        let pos = origin + at.to_vec2() - Vec2::new(60.0, 20.0);
        let Ok(exe) = std::env::current_exe() else { return };
        let spawned = std::process::Command::new(exe)
            .arg("--window")
            .arg(&url)
            .arg("--at")
            .arg(format!("{},{}", pos.x.round(), pos.y.round()))
            .arg("--size")
            .arg(format!("{},{}", size.x.round(), size.y.round()))
            .spawn();
        if spawned.is_ok() {
            // It moved rather than closed: keep it off the "reopen closed tab" list.
            let before = self.closed.len();
            self.close(i);
            if self.closed.len() > before {
                self.closed.pop();
            }
        }
    }

    /// Drag-reorder: a tab can't cross the pinned / unpinned boundary, paired tabs don't move, and
    /// a tab dragged onto a split pair jumps over the whole pair.
    fn move_tab(&mut self, from: usize, to: usize) {
        let ids: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        let Some(to) = split::reorder_target(&ids, &self.splits, from, to) else { return };
        if from < self.tabs.len() && to < self.tabs.len() && self.tabs[from].pinned == self.tabs[to].pinned {
            self.reposition(from, to);
        }
    }

    /// Move a tab to another index, keeping the same tab active.
    fn reposition(&mut self, from: usize, to: usize) {
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        let t = self.tabs.remove(from);
        self.tabs.insert(to.min(self.tabs.len()), t);
        if let Some(i) = active_id.and_then(|id| self.tabs.iter().position(|t| t.id == id)) {
            self.active = i;
        }
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|t| t.pinned).count()
    }

    /// Pin moves the tab to the end of the pinned group; unpin to the start of the others.
    fn set_pinned(&mut self, i: usize, on: bool) {
        if self.tabs.get(i).is_none_or(|t| t.pinned == on) {
            return;
        }
        if on {
            self.end_split(self.tabs[i].id);
        }
        let n = self.pinned_count();
        self.tabs[i].pinned = on;
        self.reposition(i, if on { n } else { n - 1 });
        self.scroll_to_active = true;
    }

    fn tab_menu(&mut self, i: usize, x: f32) {
        let Some(t) = self.tabs.get(i) else { return };
        let id = t.id;
        let others = self.tabs.iter().filter(|o| o.id != id && !o.pinned).count();
        let right = self.tabs.iter().skip(i + 1).filter(|o| !o.pinned).count();
        let cur = &self.tabs[self.active];
        let cur_busy = cur.pinned || split::find(&self.splits, cur.id).is_some();
        let (split_item, split_item2) = if split::find(&self.splits, id).is_some() {
            (
                serde_json::json!({ "label": "Swap sides", "cmd": format!("tab:swap:{id}") }),
                serde_json::json!({ "label": "Close split view", "cmd": format!("tab:unsplit:{id}"), "hint": "Ctrl+Shift+S" }),
            )
        } else if i == self.active {
            (
                serde_json::json!({ "label": "Split view with new tab", "cmd": format!("tab:splitnew:{id}"), "hint": "Ctrl+Shift+S", "disabled": t.pinned }),
                serde_json::Value::Null,
            )
        } else {
            (
                serde_json::json!({ "label": "Split view with current tab", "cmd": format!("tab:split:{id}"), "disabled": t.pinned || cur_busy }),
                serde_json::Value::Null,
            )
        };
        let items = serde_json::json!([
            { "label": "Reload", "cmd": format!("tab:reload:{id}"), "hint": "Ctrl+R" },
            { "label": "Duplicate", "cmd": format!("tab:dup:{id}") },
            { "label": if t.pinned { "Unpin" } else { "Pin" }, "cmd": format!("tab:pin:{id}") },
            { "label": if t.muted { "Unmute tab" } else { "Mute tab" }, "cmd": format!("tab:mute:{id}") },
            { "sep": true },
            split_item,
            split_item2,
            { "sep": true },
            { "label": "Close", "cmd": format!("tab:close:{id}"), "hint": "Ctrl+W" },
            { "label": "Close other tabs", "cmd": format!("tab:others:{id}"), "disabled": others == 0 },
            { "label": "Close tabs to the right", "cmd": format!("tab:right:{id}"), "disabled": right == 0 },
            { "sep": true },
            { "label": "Reopen closed tab", "cmd": "tab:reopen:0", "hint": "Ctrl+Shift+T", "disabled": self.closed.is_empty() },
        ]);
        // Drop the unused second split slot.
        let items = serde_json::Value::Array(items.as_array().into_iter().flatten().filter(|v| !v.is_null()).cloned().collect());
        self.show_menu(&format!("tab:{id}"), Anchor::Left(x), items);
    }

    /// `tab:<verb>:<tab id>` commands from the tab menu.
    fn tab_cmd(&mut self, cmd: &str, ctx: &egui::Context) {
        let Some((verb, id)) = cmd.split_once(':') else { return };
        if verb == "reopen" {
            return self.act(Action::Reopen, ctx);
        }
        let Some(id) = id.parse::<u64>().ok() else { return };
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else { return };
        match verb {
            "reload" => {
                let _ = self.tabs[i].webview.reload();
            }
            "dup" => self.queue_tab(self.tabs[i].url.clone()),
            "pin" => {
                let on = !self.tabs[i].pinned;
                self.set_pinned(i, on);
            }
            "mute" => wv2::set_muted(&self.tabs[i].webview, !self.tabs[i].muted),
            "split" => self.start_split(Some(i)),
            "splitnew" => {
                self.select(i);
                self.start_split(None);
            }
            "swap" => self.swap_split(id),
            "unsplit" => self.end_split(id),
            "close" => self.close(i),
            "others" | "right" => {
                // Pinned tabs stay; close by id since indices shift as tabs go.
                let doomed: Vec<u64> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(k, t)| !t.pinned && t.id != id && (verb == "others" || *k > i))
                    .map(|(_, t)| t.id)
                    .collect();
                for d in doomed {
                    if let Some(k) = self.tabs.iter().position(|t| t.id == d) {
                        self.close(k);
                    }
                }
            }
            _ => {}
        }
    }

    fn active_wv(&self) -> Option<&WebView> {
        self.tabs.get(self.active).map(|t| &t.webview)
    }

    fn is_bookmarked(&self) -> bool {
        self.tabs.get(self.active).is_some_and(|t| self.bookmarks.iter().any(|b| b.url == t.url))
    }

    /// The star (or Ctrl+D): open the bookmark dialog under the star, asking for a name. For a page
    /// that's already bookmarked it edits that bookmark (rename, icon-only, or remove).
    fn toggle_bookmark(&mut self) {
        let Some(t) = self.tabs.get(self.active) else { return };
        if is_internal(&t.url) {
            return;
        }
        self.panel_touched = true;
        if matches!(self.panel_open.as_ref().map(|p| &p.kind), Some(PanelKind::Bookmark(_))) {
            return self.close_panel(); // a second click on the star closes it
        }
        let existing = self.bookmarks.iter().find(|b| b.url == t.url);
        let title = match existing {
            Some(b) => b.title.clone(),
            None if t.title.is_empty() => display_url(&t.url),
            None => t.title.clone(),
        };
        let query: String = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("url", &t.url)
            .append_pair("title", &title)
            .append_pair("icon", if existing.is_some_and(|b| b.icon_only) { "1" } else { "0" })
            .append_pair("saved", if existing.is_some() { "1" } else { "0" })
            .finish();
        let page = format!("http://lb.localhost/bookmark?{query}");
        self.panel_req = Some((PanelKind::Bookmark(page), Anchor::Right(self.star_right)));
    }

    /// Save (add or update) a bookmark from the dialog. An empty name means icon only.
    fn save_bookmark(&mut self, url: &str, title: &str, icon_only: bool) {
        if url.is_empty() {
            return;
        }
        let icon_only = icon_only || title.is_empty();
        let title = if title.is_empty() {
            self.tabs.iter().find(|t| t.url == url).map(|t| t.title.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| display_url(url))
        } else {
            title.to_string()
        };
        match self.bookmarks.iter_mut().find(|b| b.url == url) {
            Some(b) => {
                b.title = title;
                b.icon_only = icon_only;
            }
            None => self.bookmarks.push(store::Bookmark { title, url: url.to_string(), icon_only }),
        }
        store::save_bookmarks(&self.bookmarks);
        self.refresh_internal_pages();
    }

    /// Ask open internal pages (start page, history) to reload their data.
    fn refresh_internal_pages(&self) {
        for t in self.tabs.iter().filter(|t| t.url.starts_with(HOME)) {
            let _ = t.webview.evaluate_script("window.__lbRefresh && window.__lbRefresh()");
        }
    }

    fn zoom_step(&mut self, dir: i32) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let z = match dir {
            0 => 1.0,
            1 => ZOOM_STEPS.iter().copied().find(|&s| s > t.zoom + 0.001).unwrap_or(5.0),
            _ => ZOOM_STEPS.iter().rev().copied().find(|&s| s < t.zoom - 0.001).unwrap_or(0.25),
        };
        t.zoom = z;
        wv2::set_zoom(&t.webview, z);
    }

    fn focus_chrome(&self, ctx: &egui::Context, id: egui::Id) {
        if let Some(wv) = self.active_wv() {
            let _ = wv.focus_parent();
        }
        ctx.memory_mut(|m| m.request_focus(id));
    }

    fn act(&mut self, a: Action, ctx: &egui::Context) {
        match a {
            Action::New => self.queue_tab(HOME),
            Action::Close => self.close(self.active),
            Action::Focus => self.focus_chrome(ctx, self.address_id),
            Action::Reload => {
                if let Some(t) = self.tabs.get(self.active) {
                    let _ = if t.loading { t.webview.evaluate_script("window.stop()") } else { t.webview.reload() };
                }
            }
            Action::Back => {
                if let Some(wv) = self.active_wv() {
                    wv2::go_back(wv);
                }
            }
            Action::Forward => {
                if let Some(wv) = self.active_wv() {
                    wv2::go_forward(wv);
                }
            }
            Action::Fullscreen(on) => self.set_fullscreen(on, ctx),
            Action::Pip => {
                if let Some(wv) = self.active_wv() {
                    wv2::toggle_pip(wv);
                }
            }
            Action::Extensions => {
                self.panel_touched = true;
                if self.panel_open.as_ref().is_some_and(|p| p.kind == PanelKind::Menu) {
                    self.close_panel();
                } else {
                    self.panel_req = Some((PanelKind::Menu, Anchor::Right(self.puzzle_right)));
                }
            }
            Action::PageDown => {
                self.close_panel();
                self.blur_address = true;
            }
            Action::History => self.show_page(HISTORY_PAGE),
            Action::BookmarksBar => self.toggle_bookmarks_bar(),
            Action::Split => {
                let cur = self.tabs.get(self.active).map(|t| (t.id, t.pinned));
                if let Some(s) = self.shown_split() {
                    self.end_split(s.left);
                } else if let Some((_, false)) = cur {
                    self.start_split(None);
                }
            }
            Action::Downloads => self.show_page(DOWNLOADS_PAGE),
            Action::DevTools => {
                if let Some(wv) = self.active_wv() {
                    wv.open_devtools();
                }
            }
            Action::Print => {
                if let Some(wv) = self.active_wv() {
                    wv2::print(wv);
                }
            }
            Action::Find => {
                match &mut self.find {
                    Some(f) => f.focus = true,
                    None => self.find = Some(FindBar { text: String::new(), counts: (0, 0), focus: true }),
                }
                self.focus_chrome(ctx, self.find_id);
            }
            Action::Esc => {
                if self.find.take().is_some() {
                    if let Some(wv) = self.active_wv() {
                        wv2::find_stop(wv);
                        let _ = wv.focus();
                    }
                }
            }
            Action::Bookmark => self.toggle_bookmark(),
            Action::NextTab | Action::PrevTab if !self.tabs.is_empty() => {
                let ids: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
                self.select(split::cycle(&ids, &self.splits, self.active, a == Action::NextTab));
            }
            Action::GoTab(n) if !self.tabs.is_empty() => {
                let i = if n >= 9 { self.tabs.len() - 1 } else { n.saturating_sub(1) };
                self.select(i.min(self.tabs.len() - 1));
            }
            Action::Reopen => {
                if let Some(u) = self.closed.pop() {
                    self.queue_tab(u);
                }
            }
            Action::ZoomIn => self.zoom_step(1),
            Action::ZoomOut => self.zoom_step(-1),
            Action::ZoomReset => self.zoom_step(0),
            _ => {}
        }
    }

    fn set_fullscreen(&mut self, on: bool, ctx: &egui::Context) {
        use egui::ViewportCommand as Cmd;
        // Real OS fullscreen hides the webview (Windows presents the window directly, bypassing the
        // compositing the child webview needs). Emulate it: borderless, always-on-top, 1px short.
        if on && !self.fullscreen {
            let (mon, outer, inner) =
                ctx.input(|i| (i.viewport().monitor_size, i.viewport().outer_rect, i.viewport().inner_rect));
            if let (Some(m), Some(o), Some(i)) = (mon, outer, inner) {
                self.saved_window = Some((o.min, i.size()));
                self.was_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                if self.was_maximized {
                    ctx.send_viewport_cmd(Cmd::Maximized(false));
                }
                ctx.send_viewport_cmd(Cmd::WindowLevel(egui::WindowLevel::AlwaysOnTop));
                ctx.send_viewport_cmd(Cmd::OuterPosition(egui::Pos2::ZERO));
                ctx.send_viewport_cmd(Cmd::InnerSize(egui::vec2(m.x, m.y - 1.0)));
                self.fullscreen = true;
            }
        } else if !on && self.fullscreen {
            ctx.send_viewport_cmd(Cmd::WindowLevel(egui::WindowLevel::Normal));
            if let Some((pos, size)) = self.saved_window.take() {
                ctx.send_viewport_cmd(Cmd::InnerSize(size));
                ctx.send_viewport_cmd(Cmd::OuterPosition(pos));
            }
            if self.was_maximized {
                ctx.send_viewport_cmd(Cmd::Maximized(true));
            }
            self.fullscreen = false;
        }
        self.last_layout = None;
        ctx.request_repaint();
    }

    /// Without a native frame, resize from the window edges ourselves: show the right cursor near an
    /// edge and hand the drag to Windows.
    fn resize_edges(&self, ctx: &egui::Context, r: ERect) {
        use egui::{CursorIcon as C, ResizeDirection as D};
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let g = RESIZE_GRIP;
        let (l, rt, t, b) = (p.x < r.left() + g, p.x > r.right() - g, p.y < r.top() + g, p.y > r.bottom() - g);
        let (dir, cursor) = match (l, rt, t, b) {
            (true, _, true, _) => (D::NorthWest, C::ResizeNorthWest),
            (_, true, true, _) => (D::NorthEast, C::ResizeNorthEast),
            (true, _, _, true) => (D::SouthWest, C::ResizeSouthWest),
            (_, true, _, true) => (D::SouthEast, C::ResizeSouthEast),
            (true, ..) => (D::West, C::ResizeWest),
            (_, true, ..) => (D::East, C::ResizeEast),
            (_, _, true, _) => (D::North, C::ResizeNorth),
            (.., true) => (D::South, C::ResizeSouth),
            _ => return,
        };
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }

    fn local_cmd(&mut self, cmd: &str) {
        if let Some(engine) = cmd.strip_prefix("set:engine:") {
            if engine.starts_with("https://") {
                self.settings.engine = engine.into();
                store::save_settings(&self.settings);
            }
        } else if let Some(accent) = cmd.strip_prefix("set:accent:") {
            if theme::parse_hex(accent).is_some() {
                self.settings.accent = accent.into();
                store::save_settings(&self.settings);
            }
        } else if let Some(v) = cmd.strip_prefix("set:bmbar:") {
            self.settings.show_bookmarks_bar = v == "1";
            store::save_settings(&self.settings);
        } else if let Some(v) = cmd.strip_prefix("set:restore:") {
            self.settings.restore_session = v == "1";
            store::save_settings(&self.settings);
        } else if let Some(url) = cmd.strip_prefix("bm:remove:") {
            self.bookmarks.retain(|b| b.url != url);
            store::save_bookmarks(&self.bookmarks);
            self.refresh_internal_pages();
        } else if let Some(url) = cmd.strip_prefix("hist:remove:") {
            lock(&self.history).retain(|e| e.url != url);
            self.save_history();
            self.refresh_internal_pages();
        } else if let Some(rest) = cmd.strip_prefix("dl:") {
            self.download_cmd(rest);
        } else if cmd == "hist:clear" {
            lock(&self.history).clear();
            self.save_history();
            self.refresh_internal_pages();
        }
    }

    /// Commands from the downloads page; `<i>` indexes the saved list.
    fn download_cmd(&mut self, cmd: &str) {
        let mut list = store::downloads();
        let (verb, arg) = cmd.split_once(':').unwrap_or((cmd, ""));
        let entry = arg.parse::<usize>().ok().and_then(|i| list.get(i).cloned().map(|e| (i, e)));
        match (verb, entry) {
            ("open", Some((_, e))) => {
                let _ = std::process::Command::new("explorer").arg(&e.path).spawn();
            }
            ("show", Some((_, e))) => {
                let _ = std::process::Command::new("explorer").arg(format!("/select,{}", e.path.display())).spawn();
            }
            ("remove", Some((i, _))) => {
                list.remove(i);
                store::save_downloads(&list);
                self.refresh_internal_pages();
            }
            ("clear", _) => {
                store::save_downloads(&[]);
                self.refresh_internal_pages();
            }
            ("folder", _) => {
                let _ = std::process::Command::new("explorer").arg(wv2::downloads_dir()).spawn();
            }
            _ => {}
        }
    }

    /// Flip ad blocking for `host`: the network filter, the YouTube script's flag on that site, and
    /// a reload so the page comes back with the new setting.
    fn toggle_adblock(&mut self, host: &str) {
        if host.is_empty() {
            return;
        }
        let off = !self.settings.adblock_off.iter().any(|h| h == host);
        self.settings.adblock_off.retain(|h| h != host);
        if off {
            self.settings.adblock_off.push(host.to_string());
        }
        store::save_settings(&self.settings);
        if let Ok(mut set) = self.adblock_off.write() {
            *set = self.settings.adblock_off.iter().cloned().collect();
        }
        let js = if off {
            "try { localStorage.setItem('lb-adblock-off', '1') } catch (_) {}; location.reload()"
        } else {
            "try { localStorage.removeItem('lb-adblock-off') } catch (_) {}; location.reload()"
        };
        for t in self.tabs.iter().filter(|t| host_of(&t.url) == host) {
            let _ = t.webview.evaluate_script(js);
        }
    }

    fn save_history(&mut self) {
        store::save_history(&lock(&self.history));
        self.history_dirty = false;
    }

    /// Show an internal page: switch to a tab that has it open, or open it in a new tab.
    fn show_page(&mut self, url: &str) {
        match self.tabs.iter().position(|t| t.url.starts_with(url)) {
            Some(i) => self.select(i),
            None => self.queue_tab(url),
        }
    }

    fn on_download(&mut self, url: String, path: Option<PathBuf>, ok: bool, now: f64) {
        if let Some(p) = &path {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut list = store::downloads();
            store::add_download(&mut list, store::DownloadEntry { name, path: p.clone(), url, time: store::now_secs(), ok });
            store::save_downloads(&list);
            self.refresh_internal_pages();
        }
        let name = path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        let text = match (ok, &name) {
            (true, Some(n)) => format!("Downloaded {n}"),
            (true, None) => "Download finished".into(),
            (false, Some(n)) => format!("Download failed: {n}"),
            (false, None) => "Download failed".into(),
        };
        self.toast = Some(Toast { text, path: path.filter(|_| ok), until: now + 10.0 });
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::Title(id, t) => {
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        if store::set_history_title(&mut lock(&self.history), &tab.url, &t) {
                            self.history_dirty = true;
                        }
                        tab.title = t;
                    }
                }
                Ev::Loaded(id, url) => {
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        tab.loading = false;
                        if !url.is_empty() {
                            tab.url = url;
                        }
                        if (tab.url.starts_with("https://") || tab.url.starts_with("http://")) && !is_internal(&tab.url) {
                            let mut h = lock(&self.history);
                            store::record_visit(&mut h, &tab.url, store::now_secs());
                            if !tab.title.is_empty() {
                                store::set_history_title(&mut h, &tab.url, &tab.title);
                            }
                            self.history_dirty = true;
                        }
                    }
                }
                Ev::MenuCmd(c) => self.menu_cmd(&c, ctx),
                Ev::Url(id, u) => {
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        tab.url = u;
                    }
                }
                Ev::Loading(id, l) => {
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        tab.loading = l;
                    }
                }
                Ev::History(id, b, f) => {
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        (tab.can_back, tab.can_fwd) = (b, f);
                    }
                }
                Ev::Favicon(id, png) => {
                    let tex = image::load_from_memory_with_format(&png, image::ImageFormat::Png).ok().map(|img| {
                        let rgba = img.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let ci = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        ctx.load_texture(format!("favicon-{id}"), ci, egui::TextureOptions::LINEAR)
                    });
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        if let (Some(t), Some(host)) = (&tex, url::Url::parse(&tab.url).ok().and_then(|u| u.host_str().map(str::to_string))) {
                            self.favicons.insert(host.trim_start_matches("www.").to_string(), t.clone());
                        }
                        tab.favicon = tex;
                    }
                }
                Ev::Focus(id) => self.focus_side(id),
                Ev::Fullscreen(id, on) => {
                    self.fullscreen_tab = on.then_some(id);
                    self.act(Action::Fullscreen(on), ctx);
                    self.show_active();
                }
                Ev::Audio(id, a, m) => {
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        (tab.audible, tab.muted) = (a, m);
                    }
                }
                Ev::Zoom(id, z) => {
                    if let Some(tab) = tab_mut(&mut self.tabs, id) {
                        tab.zoom = z;
                    }
                }
                Ev::FindCount(id, i, n) => {
                    let is_active = self.tabs.get(self.active).is_some_and(|t| t.id == id);
                    if let (true, Some(f)) = (is_active, &mut self.find) {
                        f.counts = (i, n);
                    }
                }
                Ev::NewTab(u) => self.queue_from_page(u),
                Ev::Act(a) => self.act(a, ctx),
                Ev::Downloaded(url, p, ok) => self.on_download(url, p, ok, now),
                Ev::Ext(e) => self.on_ext(e, ctx),
                Ev::ExtCmd(c) => self.ext_cmd(&c, ctx),
                Ev::LocalCmd(c) => self.local_cmd(&c),
                Ev::PanelClose => self.close_panel(),
                Ev::PanelSize(w, h) => self.resize_panel(ctx, w, h),
            }
        }
    }

    /// Persist the open tabs whenever they change (cheap: a handful of strings per frame).
    fn save_session_if_changed(&mut self) {
        // A window torn off from another doesn't own the saved session.
        if self.secondary || self.tabs.is_empty() || !self.pending.is_empty() {
            return;
        }
        let kept: Vec<&Tab> = self.tabs.iter().filter(|t| !t.url.starts_with("chrome-extension://")).collect();
        let pinned = kept.iter().take_while(|t| t.pinned).count();
        let tabs: Vec<String> = kept.iter().map(|t| t.url.clone()).collect();
        let ids: Vec<u64> = kept.iter().map(|t| t.id).collect();
        let splits = split::to_saved(&ids, &self.splits);
        let s = store::Session { active: self.active.min(tabs.len().saturating_sub(1)), tabs, pinned, splits };
        if s != self.saved_session {
            store::save_session(&s);
            self.saved_session = s;
        }
    }

    fn update_window_title(&mut self, ctx: &egui::Context) {
        let title = match self.tabs.get(self.active).map(|t| t.title.as_str()) {
            Some(t) if !t.is_empty() => format!("{t} — LightBrowser"),
            _ => "LightBrowser".to_string(),
        };
        if title != self.title_sent {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_sent = title;
        }
    }

    // ---- Extensions ------------------------------------------------------------------------

    fn ext_notify(&self, ctx: &egui::Context) -> impl Fn(extensions::ExtEvent) + Clone + Send + 'static {
        let send = self.sender(ctx);
        move |e| send(Ev::Ext(e))
    }

    fn ext_refresh(&self, ctx: &egui::Context) {
        if let Some(t) = self.tabs.first() {
            extensions::list(&t.webview, self.ext_notify(ctx));
        }
    }

    fn ext_set_status(&mut self, msg: impl Into<String>, err: bool) {
        self.ext_status = (msg.into(), err);
        self.ext_render();
    }

    /// Push the current extension list into any open extensions pages.
    fn ext_render(&self) {
        let items: Vec<_> = self
            .ext_items
            .iter()
            .filter(|(id, _, _)| !extensions::is_builtin(id))
            .map(|(id, name, enabled)| {
                let (popup, options) =
                    self.ext_folders.get(id).map(|f| extensions::manifest_pages(f)).unwrap_or((None, None));
                let folder = self.ext_folders.get(id);
                let icon = folder.and_then(|f| f.file_name()).map(|n| format!("/ext-icon/{}", n.to_string_lossy()));
                let d = folder.map(|f| extensions::details(f)).unwrap_or_default();
                serde_json::json!({
                    "id": id, "name": name, "enabled": enabled, "popup": popup, "options": options, "icon": icon,
                    "version": d.version, "description": d.description, "permissions": d.permissions,
                    "store_id": d.store_id, "pinned": self.settings.pinned.contains(id),
                })
            })
            .collect();
        let data = serde_json::json!({ "items": items, "status": self.ext_status.0, "error": self.ext_status.1 });
        let js = format!("window.__lbExtRender && window.__lbExtRender({data})");
        for t in self.tabs.iter().filter(|t| t.url.starts_with(EXT_PAGE)) {
            let _ = t.webview.evaluate_script(&js);
        }
        if let (Some(wv), Some(OpenPanel { kind: PanelKind::Menu, .. })) = (&self.panel, &self.panel_open) {
            let _ = wv.evaluate_script(&js);
        }
    }

    // ---- Dropdown panel --------------------------------------------------------------------

    fn close_panel(&mut self) {
        if let Some(open) = self.panel_open.take() {
            if let Some(wv) = &self.panel {
                let _ = wv.set_visible(false);
                let _ = wv.load_url("about:blank"); // stop the popup's scripts
            }
            // A menu or popup had keyboard focus; give it back to the page so it isn't left in a
            // hidden window (which swallows the next click). Suggestions never took focus.
            if open.kind != PanelKind::Suggest {
                if let Some(t) = self.tabs.get(self.active) {
                    let _ = t.webview.focus();
                }
            }
            self.panel_loaded.clear();
            self.panel_data = None;
        }
    }

    fn panel_is(&self, kind: &PanelKind) -> bool {
        self.panel_open.as_ref().is_some_and(|p| &p.kind == kind)
    }

    fn panel_rect(&self, ctx: &egui::Context, p: &OpenPanel) -> ERect {
        let r = ctx.content_rect();
        let under_bar = matches!(&p.kind, PanelKind::Context(tag) if tag.starts_with("bm:"));
        let top = self.frame_pad + if under_bar { self.chrome_h() } else { CHROME_H } - 6.0;
        let w = match p.anchor {
            Anchor::Span(_, w) => w,
            _ => p.size.x,
        }
        .min(r.width() - 2.0 * INSET);
        let h = p.size.y.min(r.height() - top - INSET);
        let want_left = match p.anchor {
            Anchor::Right(x) => x - w,
            Anchor::Left(x) | Anchor::Span(x, _) => x,
        };
        let left = want_left.clamp(INSET, (r.width() - INSET - w).max(INSET));
        ERect::from_min_size(egui::pos2(left, top), Vec2::new(w, h))
    }

    fn place_panel(&self, ctx: &egui::Context) {
        let (Some(wv), Some(p)) = (&self.panel, &self.panel_open) else { return };
        let rect = self.panel_rect(ctx, p);
        let ppp = ctx.pixels_per_point();
        let _ = wv.set_bounds(Self::bounds(rect));
        wv2::round_corners(wv, (rect.width() * ppp).round() as i32, (rect.height() * ppp).round() as i32, (12.0 * ppp).round() as i32);
    }

    fn resize_panel(&mut self, ctx: &egui::Context, w: f32, h: f32) {
        if let Some(p) = &mut self.panel_open {
            p.size = Vec2::new(w.clamp(160.0, 800.0), h.clamp(30.0, 600.0));
            self.place_panel(ctx);
        }
    }

    /// Send the current menu / suggestion data to the menu page (if it's the one showing).
    fn push_panel_data(&self) {
        if let (Some(wv), Some(d), true) = (&self.panel, &self.panel_data, self.panel_loaded == MENU_PAGE) {
            let _ = wv.evaluate_script(&format!("window.__lbMenu && window.__lbMenu({d})"));
        }
    }

    /// Show a context menu (items as JSON objects, see menu.html) under the toolbar at `anchor`.
    fn show_menu(&mut self, tag: &str, anchor: Anchor, items: serde_json::Value) {
        self.panel_touched = true;
        let kind = PanelKind::Context(tag.into());
        if self.panel_is(&kind) {
            self.close_panel(); // a second right-click on the same thing closes it
            return;
        }
        self.panel_data = Some(serde_json::json!({ "mode": "menu", "items": items }).to_string());
        self.panel_req = Some((kind, anchor));
    }

    /// Open (or retarget) the dropdown panel; creates its webview on first use.
    fn open_panel(&mut self, frame: &eframe::Frame, ctx: &egui::Context, kind: PanelKind, anchor: Anchor) {
        let url = match &kind {
            PanelKind::Menu => EXT_MENU.to_string(),
            PanelKind::Context(_) | PanelKind::Suggest => MENU_PAGE.to_string(),
            PanelKind::Bookmark(page) => page.clone(),
            PanelKind::Popup(id) => {
                let Some(popup) = self.ext_folders.get(id).and_then(|f| extensions::manifest_pages(f).0) else { return };
                format!("chrome-extension://{id}/{}", popup.trim_start_matches('/'))
            }
        };
        let size = match kind {
            PanelKind::Menu => Vec2::new(320.0, 60.0),
            PanelKind::Popup(_) => Vec2::new(340.0, 60.0),
            PanelKind::Bookmark(_) => Vec2::new(320.0, 200.0),
            _ => Vec2::new(220.0, 40.0),
        };
        // Suggestions must leave keyboard focus in the address bar.
        let take_focus = kind != PanelKind::Suggest;
        let reuse_page = kind.uses_menu_page() && self.panel_loaded == url;
        self.panel_open = Some(OpenPanel { kind, anchor, size });
        if self.panel.is_none() {
            let send = self.sender(ctx);
            let s_win = send.clone();
            let history = self.history.clone();
            let built = WebViewBuilder::new_with_web_context(&mut self.web_context)
                .with_url(&url)
                .with_bounds(Self::bounds(ERect::from_min_size(egui::Pos2::ZERO, size)))
                .with_initialization_script(PANEL_SCRIPT)
                // Don't grab focus on creation: suggestions must keep it in the address bar
                // (menus and popups are focused explicitly below).
                .with_focused(false)
                .with_browser_extensions_enabled(true)
                .with_custom_protocol("lb".into(), move |_id, req| serve_local(req.uri().path(), &history))
                .with_new_window_req_handler(move |u, _| {
                    s_win(Ev::NewTab(u));
                    s_win(Ev::PanelClose);
                    NewWindowResponse::Deny
                })
                .with_ipc_handler(move |req| {
                    let body = req.body();
                    let ours = req.uri().host() == Some("lb.localhost");
                    if body == "panel:close" {
                        send(Ev::PanelClose);
                    } else if let Some((w, h)) = body.strip_prefix("panel:size:").and_then(|s| s.split_once(':')) {
                        if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                            send(Ev::PanelSize(w, h));
                        }
                    } else if let (Some(cmd), true) = (body.strip_prefix("ext:"), ours) {
                        send(Ev::ExtCmd(cmd.to_string()));
                    } else if let (Some(cmd), true) = (body.strip_prefix("menu:"), ours) {
                        send(Ev::MenuCmd(cmd.to_string()));
                    }
                })
                .build_as_child(frame);
            match built {
                Ok(wv) => self.panel = Some(wv),
                Err(e) => {
                    eprintln!("failed to create panel: {e}");
                    self.panel_open = None;
                    return;
                }
            }
        } else if let (false, Some(wv)) = (reuse_page, &self.panel) {
            let _ = wv.load_url(&url);
        }
        self.panel_loaded = url;
        if reuse_page {
            self.push_panel_data();
        }
        if let Some(wv) = &self.panel {
            let _ = wv.set_visible(true);
            wv2::bring_to_front(wv);
            if take_focus {
                let _ = wv.focus();
            }
        }
        self.place_panel(ctx);
    }

    /// A command from the shared menu page.
    fn menu_cmd(&mut self, cmd: &str, ctx: &egui::Context) {
        if cmd == "ready" {
            return self.push_panel_data();
        }
        if let Some(rest) = cmd.strip_prefix("tab:") {
            self.close_panel();
            return self.tab_cmd(rest, ctx);
        }
        if let Some(rest) = cmd.strip_prefix("bm:") {
            self.close_panel();
            return self.bookmark_cmd(rest, ctx);
        }
        if let Some(i) = cmd.strip_prefix("sug:go:").and_then(|i| i.parse::<usize>().ok()) {
            if let Some(url) = self.sugs.get(i).map(|s| s.url.clone()) {
                self.close_panel();
                self.sug_close_at = None;
                self.navigate(&url);
            }
            return;
        }
        self.close_panel();
    }

    /// Load `url` in the active tab and give the page focus.
    fn navigate(&mut self, url: &str) {
        self.blur_address = true;
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.loading = true;
            let _ = t.webview.load_url(url);
            let _ = t.webview.focus();
        }
    }

    // ---- Address-bar suggestions -----------------------------------------------------------

    fn update_suggestions(&mut self, pill: ERect) {
        let sugs = suggest::suggest(&self.address, &lock(&self.history), &self.bookmarks, &self.settings.engine, store::now_secs());
        self.sugs = sugs;
        self.sug_sel = 0;
        if self.sugs.is_empty() {
            if self.panel_is(&PanelKind::Suggest) {
                self.close_panel();
            }
            return;
        }
        self.panel_data = Some(self.suggest_json());
        if self.panel_is(&PanelKind::Suggest) {
            self.push_panel_data();
        } else {
            self.panel_req = Some((PanelKind::Suggest, Anchor::Span(pill.left(), pill.width())));
        }
    }

    fn suggest_json(&self) -> String {
        serde_json::json!({ "mode": "suggest", "items": self.sugs, "sel": self.sug_sel }).to_string()
    }

    fn move_suggestion(&mut self, down: bool) {
        let n = self.sugs.len();
        if n == 0 {
            return;
        }
        self.sug_sel = if down { (self.sug_sel + 1) % n } else { (self.sug_sel + n - 1) % n };
        self.panel_data = Some(self.suggest_json());
        self.push_panel_data();
    }

    /// Icon texture for a pinned extension (loaded once from its manifest's PNG icon).
    fn ext_icon(&mut self, ctx: &egui::Context, id: &str) -> Option<TextureHandle> {
        if let Some(t) = self.ext_icons.get(id) {
            return t.clone();
        }
        let tex = self
            .ext_folders
            .get(id)
            .and_then(|f| f.file_name())
            .and_then(|n| extensions::icon(&n.to_string_lossy()))
            .filter(|(_, mime)| *mime == "image/png")
            .and_then(|(bytes, _)| image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok())
            .map(|img| {
                let img = img.resize(48, 48, image::imageops::FilterType::Triangle).to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                ctx.load_texture(format!("ext-{id}"), ci, egui::TextureOptions::LINEAR)
            });
        self.ext_icons.insert(id.to_string(), tex.clone());
        tex
    }

    fn show_ext_page(&mut self) {
        self.show_page(EXT_PAGE);
    }

    fn on_ext(&mut self, e: extensions::ExtEvent, ctx: &egui::Context) {
        use extensions::ExtEvent::*;
        match e {
            Loaded { id, folder } => {
                if self.ext_status.0.ends_with('…') {
                    self.ext_status = (String::new(), false); // "Installing…/Updating…" is done
                }
                self.ext_icons.remove(&id);
                self.ext_folders.insert(id, folder);
                self.ext_refresh(ctx);
            }
            LoadFailed { folder, error } => {
                let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.ext_set_status(format!("Couldn't load \"{name}\": {error}"), true);
            }
            List(items) => {
                self.ext_items = items;
                self.ext_render();
            }
            Changed => self.ext_refresh(ctx),
            Downloaded(Ok(folder)) => {
                self.ext_set_status("Installed. Loading the extension…", false);
                if let Some(t) = self.tabs.first() {
                    extensions::add(&t.webview, &folder, self.ext_notify(ctx));
                }
                self.show_ext_page();
            }
            Downloaded(Err(err)) => {
                self.ext_set_status(err, true);
                self.show_ext_page();
            }
        }
    }

    fn ext_cmd(&mut self, cmd: &str, ctx: &egui::Context) {
        if self.tabs.is_empty() {
            return;
        }
        let (verb, arg) = cmd.split_once(':').unwrap_or((cmd, ""));
        match verb {
            "list" => self.ext_refresh(ctx),
            "folder" => {
                let _ = std::process::Command::new("explorer").arg(extensions::dir()).spawn();
            }
            "reload" => extensions::load_all(&self.tabs[0].webview, self.ext_notify(ctx)),
            "install" => match extensions::store_id(arg) {
                Some(id) => {
                    self.ext_set_status("Downloading from the Chrome Web Store…", false);
                    let notify = self.ext_notify(ctx);
                    std::thread::spawn(move || notify(extensions::ExtEvent::Downloaded(extensions::download(&id))));
                }
                None => self.ext_set_status("That doesn't look like a Chrome Web Store link or extension ID.", true),
            },
            "toggle" => {
                if let Some((id, on)) = arg.rsplit_once(':') {
                    extensions::set_enabled(&self.tabs[0].webview, id.to_string(), on == "1", self.ext_notify(ctx));
                }
            }
            "remove" => {
                let folder = self.ext_folders.get(arg).cloned();
                self.settings.pinned.retain(|p| p != arg);
                store::save_settings(&self.settings);
                extensions::remove(&self.tabs[0].webview, arg.to_string(), folder, self.ext_notify(ctx));
            }
            "popup" => {
                self.panel_touched = true;
                let anchor = self.pin_anchors.get(arg).copied().unwrap_or(self.puzzle_right);
                let kind = PanelKind::Popup(arg.to_string());
                if self.panel_open.as_ref().is_some_and(|p| p.kind == kind) {
                    self.close_panel();
                } else {
                    self.panel_req = Some((kind, Anchor::Right(anchor)));
                }
            }
            "pin" => {
                if let Some((id, on)) = arg.rsplit_once(':') {
                    self.settings.pinned.retain(|p| p != id);
                    if on == "1" {
                        self.settings.pinned.push(id.to_string());
                    }
                    store::save_settings(&self.settings);
                    self.ext_render();
                }
            }
            "manage" => {
                self.close_panel();
                self.show_ext_page();
            }
            "update" => {
                let folder = self.ext_folders.get(arg).cloned();
                if let Some(store_id) = folder.as_deref().and_then(|f| extensions::details(f).store_id) {
                    self.ext_set_status("Updating from the Chrome Web Store…", false);
                    // Unregister (keeping the files), then re-download over the same folder and re-add.
                    extensions::remove(&self.tabs[0].webview, arg.to_string(), None, self.ext_notify(ctx));
                    let notify = self.ext_notify(ctx);
                    std::thread::spawn(move || notify(extensions::ExtEvent::Downloaded(extensions::download(&store_id))));
                }
            }
            "open" if arg.starts_with("chrome-extension://") => self.queue_tab(arg.to_string()),
            _ => {}
        }
    }

    // ---- Bookmarks bar ---------------------------------------------------------------------

    /// The bookmarks bar shows on the home page only, so other pages get the full height.
    fn bar_shown(&self) -> bool {
        self.settings.show_bookmarks_bar && !self.fullscreen && self.tabs.get(self.active).is_some_and(|t| t.url == HOME)
    }

    /// Height of the toolbar (tab row, navigation row, and the bookmarks bar when shown).
    fn chrome_h(&self) -> f32 {
        CHROME_H + if self.bar_shown() { BM_BAR_H } else { 0.0 }
    }

    fn toggle_bookmarks_bar(&mut self) {
        self.settings.show_bookmarks_bar = !self.settings.show_bookmarks_bar;
        store::save_settings(&self.settings);
        self.close_panel();
    }

    fn bookmark_menu(&mut self, i: usize, x: f32) {
        let icon_only = self.bookmarks.get(i).is_some_and(|b| b.icon_only);
        let items = serde_json::json!([
            { "label": "Open", "cmd": format!("bm:open:{i}") },
            { "label": "Open in new tab", "cmd": format!("bm:newtab:{i}") },
            { "sep": true },
            { "label": "Rename…", "cmd": format!("bm:rename:{i}"), "hint": "Empty = icon only" },
            { "label": if icon_only { "Show name" } else { "Show icon only" }, "cmd": format!("bm:icon:{i}") },
            { "label": "Remove", "cmd": format!("bm:del:{i}"), "danger": true },
            { "sep": true },
            { "label": "Hide bookmarks bar", "cmd": "bm:hidebar", "hint": "Ctrl+Shift+B" },
        ]);
        self.show_menu(&format!("bm:{i}"), Anchor::Left(x), items);
    }

    /// The `»` list of bookmarks that didn't fit.
    fn bookmark_overflow(&mut self, from: usize, right: f32) {
        let items: Vec<_> = self.bookmarks[from..]
            .iter()
            .enumerate()
            .map(|(k, b)| {
                let title = if b.title.is_empty() { display_url(&b.url) } else { b.title.clone() };
                serde_json::json!({ "label": title, "cmd": format!("bm:open:{}", from + k), "icon": "favicon", "url": b.url })
            })
            .collect();
        self.show_menu("bm:more", Anchor::Right(right), serde_json::Value::Array(items));
    }

    /// `bm:*` commands from the bookmark context menu / overflow list.
    fn bookmark_cmd(&mut self, cmd: &str, ctx: &egui::Context) {
        if cmd == "hidebar" {
            return self.toggle_bookmarks_bar();
        }
        if let Some(json) = cmd.strip_prefix("save:") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                let (url, title) = (v["url"].as_str().unwrap_or_default(), v["title"].as_str().unwrap_or_default());
                self.save_bookmark(url, title.trim(), v["icon_only"].as_bool().unwrap_or(false));
            }
            return;
        }
        if let Some(url) = cmd.strip_prefix("unsave:") {
            self.bookmarks.retain(|b| b.url != url);
            store::save_bookmarks(&self.bookmarks);
            self.refresh_internal_pages();
            return;
        }
        let Some((verb, i)) = cmd.split_once(':') else { return };
        let Some((i, b)) = i.parse::<usize>().ok().and_then(|i| self.bookmarks.get(i).map(|b| (i, b.clone()))) else { return };
        match verb {
            "open" => self.navigate(&b.url),
            "newtab" => self.queue_tab(b.url),
            "rename" => {
                self.bm_rename = Some((i, b.title, true));
                if let Some(wv) = self.active_wv() {
                    let _ = wv.focus_parent(); // typing goes to our window, not the page
                }
                ctx.request_repaint();
            }
            "icon" => {
                if let Some(b) = self.bookmarks.get_mut(i) {
                    b.icon_only = !b.icon_only;
                }
                store::save_bookmarks(&self.bookmarks);
                self.refresh_internal_pages();
            }
            "del" => {
                self.bookmarks.remove(i);
                store::save_bookmarks(&self.bookmarks);
                self.refresh_internal_pages();
            }
            _ => {}
        }
    }

    fn bookmark_favicon(&self, url: &str) -> Option<&TextureHandle> {
        let host = url::Url::parse(url).ok()?.host_str()?.trim_start_matches("www.").to_string();
        self.favicons.get(&host)
    }

    /// Bookmark chips in a row under the address bar; what doesn't fit goes behind `»`.
    fn draw_bookmarks_bar(&mut self, ui: &mut egui::Ui, pal: &Palette, area: ERect) {
        enum Do {
            Open(String),
            NewTab(String),
            Menu(usize, f32),
            More(usize, f32),
            Move(usize, usize),
            /// Finish renaming bookmark `.0` to `.1`; `.2` = save (Enter) rather than cancel.
            Rename(usize, String, bool),
        }
        let mut todo = None;
        let p = ui.painter().clone();
        let font = FontId::proportional(12.5);
        let (h, pad, icon, gap) = (26.0, 8.0, 16.0, 2.0);
        let y = area.center().y;

        if self.bookmarks.is_empty() {
            let hint = "Bookmark pages with ☆ (Ctrl+D) to see them here";
            p.text(egui::pos2(area.left() + 8.0, y), Align2::LEFT_CENTER, hint, font, pal.muted);
            return;
        }

        // Lay the chips out, leaving room for the `»` button if they don't all fit.
        let more_w = 30.0;
        let widths: Vec<f32> = self
            .bookmarks
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if self.bm_rename.as_ref().is_some_and(|r| r.0 == i) {
                    return 200.0;
                }
                if b.icon_only {
                    return h; // square: just the icon
                }
                let title = if b.title.is_empty() { display_url(&b.url) } else { b.title.clone() };
                let text_w = p.layout_no_wrap(title, font.clone(), pal.text).size().x.min(140.0);
                pad + icon + 6.0 + text_w + pad
            })
            .collect();
        let total: f32 = widths.iter().map(|w| w + gap).sum();
        let limit = if total > area.width() { area.width() - more_w - gap } else { area.width() };
        let mut x = area.left();
        let mut rects = vec![];
        for w in &widths {
            if x + w > area.left() + limit {
                break;
            }
            rects.push(ERect::from_min_size(egui::pos2(x, y - h / 2.0), Vec2::new(*w, h)));
            x += w + gap;
        }
        let shown = rects.len();
        let centers: Vec<f32> = rects.iter().map(|r| r.center().x).collect();

        for (i, rect) in rects.iter().enumerate() {
            let b = &self.bookmarks[i];
            let title = if b.title.is_empty() { display_url(&b.url) } else { b.title.clone() };

            // Inline rename.
            if let Some((ri, text, first)) = self.bm_rename.as_mut().filter(|r| r.0 == i) {
                let id = egui::Id::unique("bm-rename");
                p.rect_filled(*rect, CornerRadius::same(8), pal.card);
                p.rect_stroke(*rect, CornerRadius::same(8), Stroke::new(1.5, pal.accent), egui::StrokeKind::Inside);
                let resp = ui.put(
                    rect.shrink2(Vec2::new(8.0, 4.0)),
                    egui::TextEdit::singleline(text).id(id).frame(egui::Frame::NONE).font(font.clone()).margin(Margin::ZERO),
                );
                if *first {
                    resp.request_focus();
                    let len = text.chars().count();
                    let mut st = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(len),
                    )));
                    st.store(ui.ctx(), id);
                    *first = false;
                } else if resp.lost_focus() {
                    let save = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    todo = Some(Do::Rename(*ri, text.clone(), save));
                }
                continue;
            }

            let resp = ui.interact(*rect, egui::Id::unique(("bm-chip", &b.url)), egui::Sense::click_and_drag());
            if resp.hovered() || resp.dragged() {
                p.rect_filled(*rect, CornerRadius::same(8), if resp.dragged() { pal.card } else { pal.hover });
            }
            let ic_x = if b.icon_only { rect.center().x } else { rect.left() + pad + icon / 2.0 };
            let ic = ERect::from_center_size(egui::pos2(ic_x, y), Vec2::splat(icon));
            match self.bookmark_favicon(&b.url) {
                Some(t) => {
                    p.image(t.id(), ic, ERect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
                }
                None => {
                    p.rect_filled(ic, CornerRadius::same(4), pal.accent.gamma_multiply(0.3));
                    let letter = title.chars().next().unwrap_or('?').to_uppercase().to_string();
                    p.text(ic.center(), Align2::CENTER_CENTER, letter, FontId::proportional(10.0), pal.text);
                }
            }
            if !b.icon_only {
                let text_rect = ERect::from_min_max(egui::pos2(ic.right() + 6.0, rect.top()), egui::pos2(rect.right() - pad, rect.bottom()));
                let galley = p.layout_no_wrap(title.clone(), font.clone(), pal.text);
                let clipped = galley.size().x > text_rect.width();
                // Leave room for the "…" so it doesn't sit on top of the last letters.
                let clip = if clipped { text_rect.with_max_x(text_rect.right() - 10.0) } else { text_rect };
                p.with_clip_rect(clip).galley(egui::pos2(text_rect.left(), y - galley.size().y / 2.0), galley, pal.text);
                if clipped {
                    p.text(egui::pos2(text_rect.right(), y), Align2::RIGHT_CENTER, "…", font.clone(), pal.muted);
                }
            }

            if resp.dragged() {
                if let Some(to) = resp.interact_pointer_pos().and_then(|pos| chrome::reorder_target(i, pos.x, &centers)) {
                    todo = Some(Do::Move(i, to));
                }
            } else if resp.clicked() {
                todo = Some(Do::Open(b.url.clone()));
            } else if resp.middle_clicked() {
                todo = Some(Do::NewTab(b.url.clone()));
            } else if resp.secondary_clicked() {
                todo = Some(Do::Menu(i, rect.left()));
            }
            if !resp.dragged() {
                resp.on_hover_text(format!("{title}\n{}", b.url));
            }
        }

        if shown < self.bookmarks.len() {
            let r = ERect::from_min_size(egui::pos2(area.left() + limit + gap, y - h / 2.0), Vec2::new(more_w, h));
            let resp = ui.interact(r, egui::Id::unique("bm-more"), egui::Sense::click()).on_hover_text("More bookmarks");
            if resp.hovered() {
                p.rect_filled(r, CornerRadius::same(8), pal.hover);
            }
            p.text(r.center(), Align2::CENTER_CENTER, "»", FontId::proportional(16.0), pal.text);
            if resp.clicked() {
                todo = Some(Do::More(shown, r.right()));
            }
        }

        match todo {
            Some(Do::Open(url)) => self.navigate(&url),
            Some(Do::NewTab(url)) => self.queue_tab(url),
            Some(Do::Menu(i, x)) => self.bookmark_menu(i, x),
            Some(Do::More(from, right)) => {
                self.panel_touched = true;
                if self.panel_is(&PanelKind::Context("bm:more".into())) {
                    self.close_panel();
                } else {
                    self.bookmark_overflow(from, right);
                }
            }
            Some(Do::Move(from, to)) => {
                let b = self.bookmarks.remove(from);
                self.bookmarks.insert(to, b);
                store::save_bookmarks(&self.bookmarks);
                self.refresh_internal_pages();
            }
            Some(Do::Rename(i, title, save)) => {
                self.bm_rename = None;
                let title = title.trim();
                if save {
                    if let Some(b) = self.bookmarks.get_mut(i) {
                        // Clearing the name keeps the old one as the tooltip and shows just the icon.
                        if title.is_empty() {
                            b.icon_only = true;
                        } else {
                            b.title = title.to_string();
                            b.icon_only = false;
                        }
                        store::save_bookmarks(&self.bookmarks);
                        self.refresh_internal_pages();
                    }
                }
            }
            None => {}
        }
    }

    // ---- Drawing ---------------------------------------------------------------------------

    fn shortcut(&self, ctx: &egui::Context) -> Option<Action> {
        use egui::Key;
        ctx.input(|i| {
            let (c, s, alt) = (i.modifiers.command, i.modifiers.shift, i.modifiers.alt);
            let k = |key| i.key_pressed(key);
            let nums = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
            Some(if c && s && k(Key::T) {
                Action::Reopen
            } else if c && s && k(Key::Tab) {
                Action::PrevTab
            } else if c && s && k(Key::B) {
                Action::BookmarksBar
            } else if c && s && k(Key::S) {
                Action::Split
            } else if c && s && k(Key::P) {
                Action::Pip
            } else if c && k(Key::Tab) {
                Action::NextTab
            } else if c && k(Key::T) {
                Action::New
            } else if c && k(Key::W) {
                Action::Close
            } else if c && k(Key::L) {
                Action::Focus
            } else if c && k(Key::J) {
                Action::Downloads
            } else if c && k(Key::P) {
                Action::Print
            } else if k(Key::F12) {
                Action::DevTools
            } else if k(Key::F5) || (c && k(Key::R)) {
                Action::Reload
            } else if c && k(Key::F) {
                Action::Find
            } else if c && k(Key::H) {
                Action::History
            } else if c && k(Key::D) {
                Action::Bookmark
            } else if c && (k(Key::Equals) || k(Key::Plus)) {
                Action::ZoomIn
            } else if c && k(Key::Minus) {
                Action::ZoomOut
            } else if c && k(Key::Num0) {
                Action::ZoomReset
            } else if let Some(n) = nums.iter().position(|&key| c && k(key)) {
                Action::GoTab(n + 1)
            } else if alt && k(Key::ArrowLeft) {
                Action::Back
            } else if alt && k(Key::ArrowRight) {
                Action::Forward
            } else if k(Key::Escape) && self.find.is_some() {
                Action::Esc
            } else {
                return None;
            })
        })
    }

    /// The toolbar: a tab row and a navigation row, drawn at fixed positions for a precise layout.
    fn draw_toolbar(&mut self, ui: &mut egui::Ui, pal: &Palette, full: ERect, out: &mut Vec<UiReq>) {
        let row = |y: f32, h: f32| ERect::from_min_size(egui::pos2(full.left() + INSET, full.top() + y), Vec2::new(full.width() - 2.0 * INSET, h));
        let lr = || egui::Layout::left_to_right(egui::Align::Center);

        // Empty toolbar space moves the window (there is no title bar). Registered first, so every
        // widget drawn on top of it takes priority.
        let bar = ERect::from_min_size(full.min, Vec2::new(full.width(), self.chrome_h()));
        let drag = ui.interact(bar, egui::Id::unique("window-drag"), egui::Sense::click_and_drag());
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        if drag.double_clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        } else if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        // Window buttons, top-right.
        let controls = ERect::from_min_size(egui::pos2(full.right() - 3.0 * WIN_BTN_W, full.top()), Vec2::new(3.0 * WIN_BTN_W, WIN_BTN_H));
        ui.scope_builder(egui::UiBuilder::new().max_rect(controls).layout(lr()), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if chrome::window_button(ui, pal, chrome::WinBtn::Minimize).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            let kind = if maximized { chrome::WinBtn::Restore } else { chrome::WinBtn::Maximize };
            if chrome::window_button(ui, pal, kind).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            if chrome::window_button(ui, pal, chrome::WinBtn::Close).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        // Row 1: tabs
        let tab_row = row(8.0, TAB_H);
        ui.scope_builder(egui::UiBuilder::new().max_rect(tab_row).layout(lr()), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let titles: Vec<String> = self
                .tabs
                .iter()
                .map(|t| if t.title.is_empty() { display_url(&t.url) } else { t.title.clone() })
                .map(|t| if t.is_empty() { "New Tab".into() } else { t })
                .collect();
            let views: Vec<TabView> = self
                .tabs
                .iter()
                .zip(&titles)
                .map(|(t, title)| TabView {
                    id: t.id,
                    title,
                    favicon: t.favicon.as_ref(),
                    loading: t.loading,
                    audible: t.audible,
                    muted: t.muted,
                    pinned: t.pinned,
                    pair: match split::find(&self.splits, t.id).map(|k| self.splits[k]) {
                        Some(s) if s.left == t.id => chrome::PairPos::Left,
                        Some(_) => chrome::PairPos::Right,
                        None => chrome::PairPos::None,
                    },
                })
                .collect();
            // Leave room for the new-tab button, a strip of draggable space, and the window buttons.
            let width = (tab_row.width() - BTN - 6.0 - 3.0 * WIN_BTN_W - 48.0).max(0.0);
            let area = ERect::from_min_size(tab_row.min, Vec2::new(width, tab_row.height()));
            let (event, right) = chrome::tab_strip(ui, pal, &views, self.active, area, self.scroll_to_active);
            if let Some(e) = event {
                out.push(UiReq::Tab(e));
            }
            self.scroll_to_active = false;
            // New-tab button right after the last tab, on the same centre line.
            let plus = ERect::from_center_size(egui::pos2(right + 6.0 + BTN / 2.0, tab_row.center().y), Vec2::splat(BTN));
            let clicked = ui
                .scope_builder(egui::UiBuilder::new().max_rect(plus), |ui| {
                    chrome::icon_button(ui, pal, Icon::Plus, "New tab (Ctrl+T)", true).clicked()
                })
                .inner;
            if clicked {
                out.push(UiReq::Act(Action::New));
            }
        });

        // Row 2: navigation, address pill, tools
        let nav_row = row(48.0, PILL_H);
        ui.scope_builder(egui::UiBuilder::new().max_rect(nav_row).layout(lr()), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let (can_back, can_fwd, loading, zoom, url) = self
                .tabs
                .get(self.active)
                .map(|t| (t.can_back, t.can_fwd, t.loading, t.zoom, t.url.clone()))
                .unwrap_or((false, false, false, 1.0, String::new()));
            if chrome::icon_button(ui, pal, Icon::Back, "Back (Alt+←)", can_back).clicked() {
                out.push(UiReq::Act(Action::Back));
            }
            if chrome::icon_button(ui, pal, Icon::Forward, "Forward (Alt+→)", can_fwd).clicked() {
                out.push(UiReq::Act(Action::Forward));
            }
            let (icon, hint) = if loading { (Icon::Stop, "Stop") } else { (Icon::Reload, "Reload (Ctrl+R)") };
            if chrome::icon_button(ui, pal, icon, hint, true).clicked() {
                out.push(UiReq::Act(Action::Reload));
            }
            ui.add_space(6.0);

            // Address pill
            let pinned: Vec<(String, String)> = self
                .settings
                .pinned
                .iter()
                .filter_map(|id| self.ext_items.iter().find(|(i, _, on)| i == id && *on).map(|(i, n, _)| (i.clone(), n.clone())))
                .collect();
            let pill_w = (ui.available_width() - (pinned.len() + 1) as f32 * (BTN + 2.0) - 10.0).max(160.0);
            let (pill, _) = ui.allocate_exact_size(Vec2::new(pill_w, PILL_H), egui::Sense::hover());
            let radius = CornerRadius::same(12);
            let p = ui.painter().clone();
            p.rect_filled(pill, radius, pal.card);
            let ring = if self.addr_focused { Stroke::new(1.5, pal.accent) } else { Stroke::new(1.0, pal.border) };
            p.rect_stroke(pill, radius, ring, egui::StrokeKind::Inside);

            let secure = url.starts_with("https://");
            let lead = ERect::from_center_size(egui::pos2(pill.left() + 18.0, pill.center().y), Vec2::splat(16.0));
            let lead_icon = if secure && !self.addr_focused { Icon::Lock } else { Icon::Search };
            chrome::paint_icon(&p, lead, lead_icon, pal.muted);

            let internal = is_internal(&url);
            let zoom_w = if (zoom - 1.0).abs() > 0.001 { 50.0 } else { 0.0 };
            let star_w = if internal { 0.0 } else { 28.0 };
            let shield_w = star_w;
            let text_rect = ERect::from_min_max(
                egui::pos2(pill.left() + 34.0, pill.top() + 4.0),
                egui::pos2(pill.right() - 8.0 - zoom_w - star_w - shield_w, pill.bottom() - 4.0),
            );
            // Up/Down walk the suggestion list (taken before the text box sees them).
            if self.addr_focused && self.panel_is(&PanelKind::Suggest) {
                let (down, up) = ui.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    )
                });
                if down || up {
                    self.move_suggestion(down);
                }
            }
            let resp = ui.put(
                text_rect,
                egui::TextEdit::singleline(&mut self.address)
                    .id(self.address_id)
                    .frame(egui::Frame::NONE)
                    .align(Align2::LEFT_CENTER)
                    .font(FontId::proportional(14.0))
                    .margin(Margin::ZERO)
                    .hint_text("Search or enter address"),
            );
            if resp.gained_focus() {
                // Show the full URL and select all of it for easy replacement.
                if let Some(t) = self.tabs.get(self.active) {
                    self.address = if t.url == HOME { String::new() } else { t.url.clone() };
                }
                let len = self.address.chars().count();
                let mut st = egui::TextEdit::load_state(ui.ctx(), resp.id).unwrap_or_default();
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(len),
                )));
                st.store(ui.ctx(), resp.id);
                self.sug_close_at = None;
            }
            if resp.changed() && ui.memory(|m| m.has_focus(resp.id)) {
                self.update_suggestions(pill);
            }
            if resp.lost_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let picked = self.sugs.get(self.sug_sel).filter(|_| self.panel_is(&PanelKind::Suggest));
                    let url = picked.map(|s| s.url.clone()).unwrap_or_else(|| url_input::resolve(&self.address, &self.settings.engine));
                    out.push(UiReq::Go(url));
                } else {
                    // Clicking a suggestion takes focus first; give that click time to arrive.
                    self.sug_close_at = Some(ui.input(|i| i.time) + 0.3);
                }
                self.shown_url.clear();
            }
            // Widget focus, not `resp.has_focus()`: that also needs OS focus on our window, which a
            // child webview usually holds. Focus is dropped explicitly when the page takes over.
            self.addr_focused = ui.memory(|m| m.has_focus(resp.id));

            let mut x = pill.right() - 6.0;
            if star_w > 0.0 {
                let r = ERect::from_center_size(egui::pos2(x - 12.0, pill.center().y), Vec2::splat(24.0));
                let bookmarked = self.is_bookmarked();
                let resp = ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| chrome::star_button(ui, pal, bookmarked)).inner;
                self.star_right = resp.rect.right() + 6.0;
                if resp.clicked() {
                    out.push(UiReq::Act(Action::Bookmark));
                }
                x -= star_w;
            }
            if shield_w > 0.0 {
                let host = host_of(&url);
                let off = !host.is_empty() && self.settings.adblock_off.contains(&host);
                let r = ERect::from_center_size(egui::pos2(x - 12.0, pill.center().y), Vec2::splat(24.0));
                let hint = if off { format!("Ad blocking is off for {host}") } else { format!("Ad blocking is on for {host}") };
                let resp = ui.interact(r, egui::Id::unique("adblock-shield"), egui::Sense::click()).on_hover_text(hint);
                if resp.hovered() {
                    p.rect_filled(r, CornerRadius::same(7), pal.hover);
                }
                let (icon, color) = if off { (Icon::ShieldOff, pal.muted) } else { (Icon::Shield, pal.accent) };
                chrome::paint_icon(&p, r, icon, color);
                if resp.clicked() {
                    out.push(UiReq::ToggleAdblock(host));
                }
                x -= shield_w;
            }
            if zoom_w > 0.0 {
                let r = ERect::from_center_size(egui::pos2(x - zoom_w / 2.0, pill.center().y), Vec2::new(zoom_w - 6.0, 22.0));
                let resp = ui.interact(r, egui::Id::unique("zoom-badge"), egui::Sense::click()).on_hover_text("Reset zoom (Ctrl+0)");
                p.rect_filled(r, CornerRadius::same(11), if resp.hovered() { pal.accent.gamma_multiply(0.35) } else { pal.accent.gamma_multiply(0.2) });
                p.text(r.center(), Align2::CENTER_CENTER, format!("{:.0}%", zoom * 100.0), FontId::proportional(11.5), pal.text);
                if resp.clicked() {
                    out.push(UiReq::Act(Action::ZoomReset));
                }
            }

            // Loading shimmer along the pill's bottom edge.
            if loading {
                let t = ui.input(|i| i.time) as f32;
                let (l, r) = (pill.left() + 12.0, pill.right() - 12.0);
                let w = r - l;
                let seg = w * 0.3;
                let x0 = l + ((t * 0.8) % 1.0) * (w + seg) - seg;
                let bar = ERect::from_min_max(egui::pos2(x0.max(l), pill.bottom() - 2.5), egui::pos2((x0 + seg).min(r), pill.bottom() - 1.0));
                p.rect_filled(bar, CornerRadius::same(1), pal.accent);
                ui.ctx().request_repaint();
            }

            ui.add_space(6.0);
            for (id, name) in &pinned {
                let tex = self.ext_icon(ui.ctx(), id);
                let resp = chrome::image_button(ui, pal, tex.as_ref(), name);
                self.pin_anchors.insert(id.clone(), resp.rect.right());
                if resp.clicked() {
                    out.push(UiReq::ExtCmd(format!("popup:{id}")));
                }
            }
            let resp = chrome::icon_button(ui, pal, Icon::Puzzle, "Extensions", true);
            self.puzzle_right = resp.rect.right();
            if resp.clicked() {
                out.push(UiReq::Act(Action::Extensions));
            }
        });

        // Row 3: bookmarks bar
        if self.bar_shown() {
            self.draw_bookmarks_bar(ui, pal, row(CHROME_H - 5.0, BM_BAR_H - 2.0));
        }
    }

    /// The card under the toolbar: find bar, or a download notice.
    fn draw_strip(&mut self, ui: &mut egui::Ui, pal: &Palette, full: ERect, out: &mut Vec<UiReq>) {
        let card = ERect::from_min_size(
            egui::pos2(full.left() + INSET, full.top() + self.chrome_h() + 2.0),
            Vec2::new(full.width() - 2.0 * INSET, STRIP_H - 8.0),
        );
        let p = ui.painter().clone();
        let radius = CornerRadius::same(12);
        p.add(pal.card_shadow().as_shape(card, radius));
        p.rect_filled(card, radius, pal.card);
        let inner = card.shrink2(Vec2::new(10.0, 0.0));
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(f) = &mut self.find {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                    chrome::paint_icon(&p, r, Icon::Search, pal.muted);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut f.text)
                            .id(self.find_id)
                            .frame(egui::Frame::NONE)
                            .font(FontId::proportional(14.0))
                            .desired_width(280.0)
                            .hint_text("Find in page"),
                    );
                    if f.focus {
                        resp.request_focus();
                        f.focus = false;
                    }
                    if resp.changed() {
                        out.push(UiReq::FindStart(f.text.clone()));
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let back = ui.input(|i| i.modifiers.shift);
                        out.push(UiReq::FindStep(!back));
                        resp.request_focus();
                    }
                    let (idx, n) = f.counts;
                    let (label, color) = if f.text.is_empty() {
                        (String::new(), pal.muted)
                    } else if n <= 0 {
                        ("No results".into(), Color32::from_rgb(229, 72, 77))
                    } else {
                        (format!("{} of {n}", idx.max(1)), pal.muted)
                    };
                    ui.label(egui::RichText::new(label).color(color).size(12.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if chrome::icon_button(ui, pal, Icon::Close, "Close (Esc)", true).clicked() {
                            out.push(UiReq::Act(Action::Esc));
                        }
                        if chrome::icon_button(ui, pal, Icon::Down, "Next (Enter)", n > 0).clicked() {
                            out.push(UiReq::FindStep(true));
                        }
                        if chrome::icon_button(ui, pal, Icon::Up, "Previous (Shift+Enter)", n > 0).clicked() {
                            out.push(UiReq::FindStep(false));
                        }
                    });
                } else if let Some(t) = &self.toast {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                    chrome::paint_icon(&p, r, Icon::Download, pal.accent);
                    ui.label(egui::RichText::new(&t.text).color(pal.text).size(13.5));
                    if let Some(path) = &t.path {
                        ui.add_space(6.0);
                        if ui.link("Open").clicked() {
                            let _ = std::process::Command::new("explorer").arg(path).spawn();
                        }
                        if ui.link("Show in folder").clicked() {
                            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
                        }
                        if ui.link("All downloads").clicked() {
                            out.push(UiReq::Act(Action::Downloads));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if chrome::icon_button(ui, pal, Icon::Close, "Dismiss", true).clicked() {
                            out.push(UiReq::DismissToast);
                        }
                    });
                }
            },
        );
    }
}

/// Things the toolbar asks for, applied once drawing is done (avoids borrowing self twice).
enum UiReq {
    Act(Action),
    ExtCmd(String),
    Tab(TabEvent),
    Go(String),
    FindStart(String),
    FindStep(bool),
    DismissToast,
    /// Turn ad blocking off (or back on) for this site.
    ToggleAdblock(String),
}

impl eframe::App for BrowserApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let c = visuals.panel_fill;
        Color32::from_rgb(c.r(), c.g(), c.b()).to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);

        // Theme: install the palette when the accent changes; tint the title bar to match.
        if self.installed_accent != self.settings.accent {
            Palette::install(&ctx, theme::accent_or_default(&self.settings.accent));
            self.installed_accent = self.settings.accent.clone();
        }
        let pal = Palette::new(ctx.theme() == egui::Theme::Dark, theme::accent_or_default(&self.settings.accent));
        if self.tinted != Some(pal) {
            let rgb = |c: Color32| [c.r(), c.g(), c.b()];
            wv2::tint_title_bar(frame, rgb(pal.bg), rgb(pal.text), pal.dark);
            self.tinted = Some(pal);
        }

        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let ppp = ctx.pixels_per_point();
        self.frame_pad = if maximized && !self.fullscreen {
            wv2::maximized_overhang((96.0 * ppp).round() as u32) as f32 / ppp
        } else {
            0.0
        };

        self.drain_events(&ctx);
        if std::mem::take(&mut self.exit_fullscreen) && self.fullscreen {
            self.set_fullscreen(false, &ctx);
            self.show_active();
        }
        if std::mem::take(&mut self.blur_address) {
            ctx.memory_mut(|m| m.surrender_focus(self.address_id));
        }
        for req in std::mem::take(&mut self.pending) {
            self.open_tab(frame, &ctx, req);
        }
        if let Some((kind, anchor)) = self.panel_req.take() {
            self.open_panel(frame, &ctx, kind, anchor);
        }
        if let Some(i) = self.restore_active.take() {
            // The restored session's first tabs were pinned.
            let n = self.saved_session.pinned.min(self.tabs.len());
            for t in &mut self.tabs[..n] {
                t.pinned = true;
            }
            // Rebuild split pairs; any that no longer fit (a tab failed to open…) are skipped.
            let ids: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
            let pinned: Vec<bool> = self.tabs.iter().map(|t| t.pinned).collect();
            self.splits = split::from_saved(&ids, &pinned, &self.saved_session.splits);
            self.select(i.min(self.tabs.len().saturating_sub(1)));
            self.show_active(); // `select` does nothing if that tab is already active
        }
        self.save_session_if_changed();
        self.update_window_title(&ctx);
        if let Some(at) = self.sug_close_at {
            if now >= at {
                self.sug_close_at = None;
                if self.panel_is(&PanelKind::Suggest) && !self.addr_focused {
                    self.close_panel();
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            }
        }
        // Write history at most every few seconds (and on exit).
        if self.history_dirty {
            if now - self.history_saved_at > 5.0 {
                self.save_history();
                self.history_saved_at = now;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs(5));
            }
        }
        if self.toast.as_ref().is_some_and(|t| now > t.until) {
            self.toast = None;
        }
        if let Some(t) = &self.toast {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((t.until - now).max(0.1)));
        }

        if self.close_window {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let top = self.content_top();
        let r = ctx.content_rect();
        let page = self.page_rect(r.width(), r.height());

        // The webview keeps Win32 keyboard focus after you use the page, so clicking the chrome
        // must hand focus back to our window or typing in the address bar goes nowhere.
        if !self.fullscreen && ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !page.contains(p))) {
            if let Some(wv) = self.active_wv() {
                let _ = wv.focus_parent();
            }
        }

        // Only touch the webviews when the layout actually changed.
        let pairs: Vec<(u64, u64, i32)> =
            self.splits.iter().map(|s| (s.left, s.right, (s.ratio * 1000.0).round() as i32)).collect();
        let layout = (r.width(), r.height(), top, pairs);
        if self.last_layout.as_ref() != Some(&layout) {
            if self.last_layout.as_ref().is_some_and(|l| (l.0, l.1) != (layout.0, layout.1)) {
                self.close_panel(); // window resized
            }
            self.last_layout = Some(layout);
            let ppp = ctx.pixels_per_point();
            let radius = if self.fullscreen { 0 } else { (PAGE_RADIUS * ppp).round() as i32 };
            for t in &self.tabs {
                let rect = self.pane_of(t.id, page);
                let _ = t.webview.set_bounds(Self::bounds(rect));
                wv2::round_corners(&t.webview, (rect.width() * ppp).round() as i32, (rect.height() * ppp).round() as i32, radius);
            }
            ctx.request_repaint();
        }

        if !self.fullscreen {
            self.resize_edges(&ctx, r);
        }

        if let Some(a) = self.shortcut(&ctx) {
            self.act(a, &ctx);
        }

        // Keep the bar showing the active tab unless the user is typing in it.
        if !self.addr_focused {
            if let Some(tab) = self.tabs.get(self.active) {
                if self.shown_url != tab.url {
                    self.shown_url = tab.url.clone();
                    self.address = display_url(&tab.url);
                }
            }
        }

        if self.fullscreen {
            return;
        }

        // When maximized, Windows hangs the (invisible) frame past the screen edges; keep clear of it.
        let full = ui.max_rect().shrink(self.frame_pad);
        // Soft shadow around the page card (the page itself is drawn by WebView2 on top).
        let radius = CornerRadius::same(PAGE_RADIUS as u8);
        match self.shown_split() {
            Some(s) => self.draw_split_chrome(ui, &pal, page, s),
            None => {
                ui.painter().add(pal.card_shadow().as_shape(page, radius));
            }
        }

        let mut out = Vec::new();
        let pressed_in_chrome = ctx.input(|i| i.pointer.any_pressed());
        self.panel_touched = false;
        self.draw_toolbar(ui, &pal, full, &mut out);
        if self.find.is_some() || self.toast.is_some() {
            self.draw_strip(ui, &pal, full, &mut out);
        }

        for req in out {
            match req {
                UiReq::Act(a) => self.act(a, &ctx),
                UiReq::ExtCmd(c) => self.ext_cmd(&c, &ctx),
                UiReq::Tab(TabEvent::Select(i)) => self.select(i),
                UiReq::Tab(TabEvent::Close(i)) => self.close(i),
                UiReq::Tab(TabEvent::Move { from, to }) => self.move_tab(from, to),
                UiReq::Tab(TabEvent::Detach(i, at)) => self.detach(&ctx, i, at),
                UiReq::Tab(TabEvent::Mute(i)) => {
                    if let Some(t) = self.tabs.get(i) {
                        wv2::set_muted(&t.webview, !t.muted);
                    }
                }
                UiReq::Tab(TabEvent::Menu(i, x)) => self.tab_menu(i, x),
                UiReq::Go(url) => {
                    if self.panel_is(&PanelKind::Suggest) {
                        self.close_panel();
                    }
                    self.navigate(&url);
                }
                UiReq::FindStart(term) => {
                    if let Some(wv) = self.active_wv() {
                        if term.is_empty() {
                            wv2::find_stop(wv);
                        } else if !wv2::find_start(wv, &term) {
                            self.toast = Some(Toast {
                                text: "Find in page needs a newer WebView2 runtime".into(),
                                path: None,
                                until: now + 6.0,
                            });
                        }
                    }
                    if let Some(f) = &mut self.find {
                        f.counts = (0, 0);
                    }
                }
                UiReq::FindStep(fwd) => {
                    if let Some(wv) = self.active_wv() {
                        wv2::find_step(wv, fwd);
                    }
                }
                UiReq::DismissToast => self.toast = None,
                UiReq::ToggleAdblock(host) => self.toggle_adblock(&host),
            }
        }
        // A click anywhere in the toolbar that wasn't on the panel's own buttons closes it.
        if pressed_in_chrome && !self.panel_touched && self.panel_open.is_some() && self.panel_req.is_none() {
            self.close_panel();
        }
        if self.panel_req.is_some() {
            ctx.request_repaint();
        }
    }
}

impl Drop for BrowserApp {
    fn drop(&mut self) {
        if self.history_dirty {
            self.save_history();
        }
    }
}

fn window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png).ok()?.to_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

/// How this process was started: normally (restore the session), or as a window torn off from
/// another one (`--window <url> [--at x,y] [--size w,h]`).
#[derive(Default)]
struct Launch {
    url: Option<String>,
    at: Option<[f32; 2]>,
    size: Option<[f32; 2]>,
}

impl Launch {
    fn parse(mut args: impl Iterator<Item = String>) -> Self {
        let pair = |s: Option<String>| -> Option<[f32; 2]> {
            let s = s?;
            let (a, b) = s.split_once(',')?;
            Some([a.trim().parse().ok()?, b.trim().parse().ok()?])
        };
        let mut l = Launch::default();
        while let Some(a) = args.next() {
            match a.as_str() {
                "--window" => l.url = args.next().filter(|u| !u.is_empty()),
                "--at" => l.at = pair(args.next()),
                "--size" => l.size = pair(args.next()).filter(|s| s[0] >= 300.0 && s[1] >= 200.0),
                _ => {}
            }
        }
        l
    }
}

fn main() -> eframe::Result {
    let launch = Launch::parse(std::env::args().skip(1));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(launch.size.unwrap_or([1100.0, 750.0]))
        .with_title("LightBrowser")
        .with_decorations(false)
        .with_resizable(true);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    if let Some(at) = launch.at {
        viewport = viewport.with_position(at);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "LightBrowser",
        options,
        Box::new(|_cc| {
            Ok(Box::new(BrowserApp::new(launch)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{Action, Launch};

    #[test]
    fn launch_args() {
        let a = |v: &[&str]| Launch::parse(v.iter().map(|s| s.to_string()));
        let l = a(&["--window", "https://x.org/", "--at", "10,20", "--size", "900,600"]);
        assert_eq!(l.url.as_deref(), Some("https://x.org/"));
        assert_eq!(l.at, Some([10.0, 20.0]));
        assert_eq!(l.size, Some([900.0, 600.0]));
        let l = a(&[]);
        assert!(l.url.is_none() && l.at.is_none() && l.size.is_none());
        assert!(a(&["--size", "10,10"]).size.is_none(), "too-small sizes are ignored");
        assert!(a(&["--at", "nope"]).at.is_none());
    }

    #[test]
    fn ipc_actions() {
        assert_eq!(Action::from_ipc("tab:3"), Some(Action::GoTab(3)));
        assert_eq!(Action::from_ipc("reopen"), Some(Action::Reopen));
        assert_eq!(Action::from_ipc("history"), Some(Action::History));
        assert_eq!(Action::from_ipc("bmbar"), Some(Action::BookmarksBar));
        assert_eq!(Action::from_ipc("downloads"), Some(Action::Downloads));
        assert_eq!(Action::from_ipc("devtools"), Some(Action::DevTools));
        assert_eq!(Action::from_ipc("tab:x"), None);
        assert_eq!(Action::from_ipc("nope"), None);
        // Pages can't switch fullscreen by message; that comes from WebView2.
        assert_eq!(Action::from_ipc("fs:1"), None);
    }

    #[test]
    fn shortcuts_need_the_token() {
        let t = "0123456789abcdef0123456789abcdef";
        assert_eq!(super::shortcut_from_ipc(&format!("k:{t}:close"), t), Some(Action::Close));
        assert_eq!(super::shortcut_from_ipc("close", t), None);
        assert_eq!(super::shortcut_from_ipc("k:wrong:close", t), None);
        assert_eq!(super::shortcut_from_ipc(&format!("k:{t}close"), t), None);
        assert_ne!(super::new_ipc_token(), super::new_ipc_token());
        assert_eq!(super::new_ipc_token().len(), 32);
    }
}
