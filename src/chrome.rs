//! Toolbar widgets: painted icons, icon buttons and the tab strip. Everything takes its colours
//! from the shared [`Palette`].

use eframe::egui::{
    self, Color32, CornerRadius, FontId, Id, Painter, Pos2, Rect, Sense, Shape, Stroke, TextureHandle, Vec2,
};

use crate::theme::Palette;

pub const BTN: f32 = 30.0;
pub const TAB_H: f32 = 30.0;
const TAB_MIN: f32 = 44.0;
const TAB_MAX: f32 = 220.0;
/// Pinned tabs are icon-only.
const PIN_W: f32 = 40.0;
const GAP: f32 = 4.0;
/// Icons are drawn at this size (points) unless their box is smaller.
const ICON_PT: f32 = 20.0;

/// Toolbar / UI icons. Each is an SVG in `assets/icons/` (24×24 grid, `currentColor` strokes).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Back,
    Forward,
    Reload,
    Stop,
    Plus,
    Close,
    Lock,
    Search,
    Up,
    Down,
    Download,
    Puzzle,
    /// A tab playing sound.
    Speaker,
    /// A muted tab.
    Muted,
    /// Ad blocking on for this site…
    Shield,
    /// …or off.
    ShieldOff,
    Star,
    StarFilled,
    Minimize,
    Maximize,
    Restore,
    WindowClose,
}

impl Icon {
    fn svg(self) -> &'static str {
        match self {
            Icon::Back => include_str!("../assets/icons/back.svg"),
            Icon::Forward => include_str!("../assets/icons/forward.svg"),
            Icon::Reload => include_str!("../assets/icons/reload.svg"),
            Icon::Stop => include_str!("../assets/icons/stop.svg"),
            Icon::Plus => include_str!("../assets/icons/plus.svg"),
            Icon::Close => include_str!("../assets/icons/close.svg"),
            Icon::Lock => include_str!("../assets/icons/lock.svg"),
            Icon::Search => include_str!("../assets/icons/search.svg"),
            Icon::Up => include_str!("../assets/icons/chevron-up.svg"),
            Icon::Down => include_str!("../assets/icons/chevron-down.svg"),
            Icon::Download => include_str!("../assets/icons/download.svg"),
            Icon::Puzzle => include_str!("../assets/icons/extensions.svg"),
            Icon::Speaker => include_str!("../assets/icons/speaker.svg"),
            Icon::Muted => include_str!("../assets/icons/muted.svg"),
            Icon::Shield => include_str!("../assets/icons/shield.svg"),
            Icon::ShieldOff => include_str!("../assets/icons/shield-off.svg"),
            Icon::Star => include_str!("../assets/icons/bookmark.svg"),
            Icon::StarFilled => include_str!("../assets/icons/bookmark-filled.svg"),
            Icon::Minimize => include_str!("../assets/icons/minimize.svg"),
            Icon::Maximize => include_str!("../assets/icons/maximize.svg"),
            Icon::Restore => include_str!("../assets/icons/restore.svg"),
            Icon::WindowClose => include_str!("../assets/icons/window-close.svg"),
        }
    }
}

/// Render an icon's SVG in white at exactly `px` × `px` (cached), so it stays crisp at any display
/// scaling and can be tinted any colour when drawn.
fn icon_texture(ctx: &egui::Context, icon: Icon, px: u32) -> Option<TextureHandle> {
    let id = Id::unique(("svg-icon", icon, px));
    if let Some(tex) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(tex);
    }
    use resvg::{tiny_skia, usvg};
    let svg = icon.svg().replace("currentColor", "#ffffff");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(px, px)?;
    let scale = px as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let image = egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data());
    let tex = ctx.load_texture(format!("icon-{icon:?}-{px}"), image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Draw `icon` centred in `rect`, tinted with `color`.
pub fn paint_icon(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height()).min(ICON_PT);
    let ppp = p.ctx().pixels_per_point();
    let px = (size * ppp).round().max(1.0) as u32;
    // Snap to whole pixels so the 1-pixel-wide details don't blur.
    let side = px as f32 / ppp;
    let min = ((rect.center() - Vec2::splat(side / 2.0)) * ppp).round() / ppp;
    let draw = Rect::from_min_size(min, Vec2::splat(side));
    if let Some(tex) = icon_texture(p.ctx(), icon, px) {
        p.image(tex.id(), draw, Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), color);
    }
}

/// Square button with a painted icon and a soft rounded hover. Disabled buttons are dimmed and
/// never report clicks.
pub fn icon_button(ui: &mut egui::Ui, pal: &Palette, icon: Icon, hint: &str, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BTN), sense);
    if enabled && resp.hovered() {
        let fill = if resp.is_pointer_button_down_on() { pal.hover.gamma_multiply(1.8) } else { pal.hover };
        ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
    }
    let color = if enabled { pal.text } else { pal.muted.gamma_multiply(0.55) };
    paint_icon(ui.painter(), rect, icon, color);
    resp.on_hover_text(hint)
}

/// Square toolbar button showing an image (a pinned extension's icon), or a dot without one.
pub fn image_button(ui: &mut egui::Ui, pal: &Palette, tex: Option<&TextureHandle>, hint: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BTN), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(9), pal.hover);
    }
    let icon = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
    match tex {
        Some(t) => {
            ui.painter().image(t.id(), icon, Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(icon, CornerRadius::same(5), pal.accent.gamma_multiply(0.35));
            let letter = hint.chars().next().unwrap_or('?').to_uppercase().to_string();
            ui.painter().text(icon.center(), egui::Align2::CENTER_CENTER, letter, FontId::proportional(11.0), pal.text);
        }
    }
    resp.on_hover_text(hint)
}

/// Bookmark star; filled with the accent colour when the page is bookmarked.
pub fn star_button(ui: &mut egui::Ui, pal: &Palette, filled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(7), pal.hover);
    }
    let (icon, color) = if filled { (Icon::StarFilled, pal.accent) } else { (Icon::Star, pal.muted) };
    paint_icon(ui.painter(), rect, icon, color);
    resp.on_hover_text(if filled { "Remove bookmark (Ctrl+D)" } else { "Bookmark this page (Ctrl+D)" })
}

/// A rotating 270° arc.
pub fn spinner(p: &Painter, center: Pos2, r: f32, time: f64, color: Color32) {
    let start = (time * 6.0) as f32;
    let pts: Vec<Pos2> = (0..=20)
        .map(|i| {
            let a = start + i as f32 / 20.0 * std::f32::consts::PI * 1.5;
            center + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(1.8, color)));
}

#[derive(Clone, Copy)]
pub enum WinBtn {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Windows-style caption button (46×36). Close turns red on hover, like the native one.
pub fn window_button(ui: &mut egui::Ui, pal: &Palette, kind: WinBtn) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(46.0, 36.0), Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered();
    let is_close = matches!(kind, WinBtn::Close);
    if hovered {
        let fill = if is_close { Color32::from_rgb(196, 43, 28) } else { pal.hover };
        p.rect_filled(rect, CornerRadius::ZERO, fill);
    }
    let color = if hovered && is_close { Color32::WHITE } else { pal.text };
    let icon = match kind {
        WinBtn::Minimize => Icon::Minimize,
        WinBtn::Maximize => Icon::Maximize,
        WinBtn::Restore => Icon::Restore,
        WinBtn::Close => Icon::WindowClose,
    };
    paint_icon(p, rect, icon, color);
    let tip = match kind {
        WinBtn::Minimize => "Minimize",
        WinBtn::Maximize => "Maximize",
        WinBtn::Restore => "Restore down",
        WinBtn::Close => "Close",
    };
    resp.on_hover_text(tip)
}

pub struct TabView<'a> {
    pub id: u64,
    pub title: &'a str,
    pub favicon: Option<&'a TextureHandle>,
    pub loading: bool,
    pub audible: bool,
    pub muted: bool,
    pub pinned: bool,
    /// Which half of a split pair this tab is (pairs are drawn as one joined tab).
    pub pair: PairPos,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PairPos {
    None,
    Left,
    Right,
}

/// Left edge of each tab in the strip: tabs are `GAP` apart, except the two halves of a pair.
pub fn tab_lefts(widths: &[f32], pairs: &[PairPos]) -> Vec<f32> {
    let mut x = 0.0;
    widths
        .iter()
        .zip(pairs)
        .map(|(w, p)| {
            let l = x;
            x += w + if *p == PairPos::Left { 0.0 } else { GAP };
            l
        })
        .collect()
}

pub enum TabEvent {
    Select(usize),
    Close(usize),
    Move { from: usize, to: usize },
    /// The speaker icon was clicked.
    Mute(usize),
    /// Right-click, at this x.
    Menu(usize, f32),
    /// Dragged out of the tab strip and dropped at this point (window coordinates): open it in a
    /// new window.
    Detach(usize, Pos2),
}

/// How far (points) above or below the tab strip a dragged tab must be to tear off.
const DETACH_DIST: f32 = 44.0;
/// How long a tab takes to glide to a new position (reorder, close, open), in seconds.
const SLIDE_TIME: f32 = 0.16;
/// How long a new tab takes to fade in, and the active/hover highlight to cross-fade.
const FADE_TIME: f64 = 0.18;

/// While dragging tab `i` with the pointer at `x`, the index it should move to (if any):
/// past the centre of a neighbour means swap with it.
pub fn reorder_target(i: usize, x: f32, centers: &[f32]) -> Option<usize> {
    if i + 1 < centers.len() && x > centers[i + 1] {
        Some(i + 1)
    } else if i > 0 && x < centers[i - 1] {
        Some(i - 1)
    } else {
        None
    }
}

/// The tab strip: the active tab is a floating card, the others are quiet text that lights up
/// on hover. Tabs shrink to fit; past the minimum width the strip scrolls with the mouse wheel.
/// Laid out by hand inside `area` (vertically centred), so it lines up exactly with its neighbours.
/// Returns the event (if any) and the strip's right edge, for placing the new-tab button.
pub fn tab_strip(
    ui: &mut egui::Ui,
    pal: &Palette,
    tabs: &[TabView],
    active: usize,
    area: Rect,
    scroll_to_active: bool,
) -> (Option<TabEvent>, f32) {
    // Pinned tabs (always first) are a fixed icon-only width; the rest share the remaining space.
    let pinned_n = tabs.iter().take_while(|t| t.pinned).count();
    let normal_n = tabs.len() - pinned_n;
    let pinned_w = pinned_n as f32 * (PIN_W + GAP);
    let tab_w = if normal_n == 0 {
        TAB_MAX
    } else {
        ((area.width() - pinned_w - GAP * normal_n as f32) / normal_n as f32).clamp(TAB_MIN, TAB_MAX)
    };
    let widths: Vec<f32> = tabs.iter().map(|t| if t.pinned { PIN_W } else { tab_w }).collect();
    let pairs: Vec<PairPos> = tabs.iter().map(|t| t.pair).collect();
    let lefts = tab_lefts(&widths, &pairs);
    let content_w = lefts.last().zip(widths.last()).map_or(0.0, |(l, w)| l + w);
    let visible_w = area.width().min(content_w);
    let strip = Rect::from_min_size(egui::pos2(area.left(), area.center().y - TAB_H / 2.0), Vec2::new(visible_w, TAB_H));
    let mut event = None;
    let time = ui.input(|i| i.time);

    // Horizontal scroll offset, kept between frames.
    let scroll_id = Id::unique("tab-scroll");
    let max_off = (content_w - visible_w).max(0.0);
    let mut off: f32 = ui.data(|d| d.get_temp(scroll_id)).unwrap_or(0.0);
    if max_off > 0.0 && ui.rect_contains_pointer(strip) {
        let d = ui.input(|i| i.smooth_scroll_delta);
        off -= d.x + d.y;
    }
    if let (true, Some(&left), Some(&w)) = (scroll_to_active, lefts.get(active), widths.get(active)) {
        if left < off {
            off = left;
        } else if left + w > off + visible_w {
            off = left + w - visible_w;
        }
    }
    off = off.clamp(0.0, max_off);
    ui.data_mut(|d| d.insert_temp(scroll_id, off));

    // Clip sideways only, so the active tab's shadow isn't cut off.
    let painter = ui.painter().with_clip_rect(strip.expand2(Vec2::new(0.0, 8.0)));
    // Positions are animated per tab (keyed by its stable id), so tabs glide when the order changes,
    // a tab closes, or a new one opens. The tab being dragged follows the pointer instead.
    let ctx = ui.ctx().clone();
    let pointer_now = ui.input(|inp| inp.pointer.latest_pos());
    let mut rects = Vec::with_capacity(tabs.len());
    let mut fade_in = Vec::with_capacity(tabs.len());
    let mut dragged_idx = None;
    for (i, t) in tabs.iter().enumerate() {
        // Stable per-tab id so a drag survives the tab moving to a new slot.
        let tab_id = Id::unique(("tab", t.id));
        let x_id = Id::unique(("tab-x", t.id));
        let born_id = Id::unique(("tab-born", t.id));
        let grab_id = Id::unique(("tab-grab", t.id));
        let target = lefts[i];
        // First sight of a tab: start it a little to the right so it slides (and fades) in.
        let born = ctx.data(|d| d.get_temp::<f64>(born_id)).unwrap_or_else(|| {
            ctx.data_mut(|d| d.insert_temp(born_id, time));
            ctx.animate_value_with_time(x_id, target + 24.0, 0.0);
            time
        });
        let mut left = ctx.animate_value_with_time(x_id, target, SLIDE_TIME);
        if ctx.is_being_dragged(tab_id) {
            if let Some(p) = pointer_now {
                // Keep the spot where the tab was grabbed under the pointer.
                let grab = ctx.data(|d| d.get_temp::<f32>(grab_id)).unwrap_or_else(|| {
                    let g = p.x - (strip.left() + left - off);
                    ctx.data_mut(|d| d.insert_temp(grab_id, g));
                    g
                });
                left = (p.x - grab - strip.left() + off).clamp(0.0, (content_w - widths[i]).max(0.0));
                // Releasing then animates from wherever the tab was dropped.
                ctx.animate_value_with_time(x_id, left, 0.0);
            }
            dragged_idx = Some(i);
        } else {
            ctx.data_mut(|d| d.remove::<f32>(grab_id));
        }
        fade_in.push(((time - born) / FADE_TIME).clamp(0.0, 1.0) as f32);
        let rect = Rect::from_min_size(egui::pos2(strip.left() + left - off, strip.top()), Vec2::new(widths[i], TAB_H));
        // Halves of a split pair can't be dragged.
        let sense = if t.pair == PairPos::None { Sense::click_and_drag() } else { Sense::click() };
        let resp = ui.interact(rect.intersect(strip), tab_id, sense);
        rects.push((rect, resp));
    }
    // Reordering compares against where tabs will settle, not where they are mid-animation.
    let centers: Vec<f32> = (0..tabs.len()).map(|i| strip.left() + lefts[i] - off + widths[i] / 2.0).collect();
    // Draw the dragged tab last, so it floats above the tabs sliding underneath it.
    let order: Vec<usize> = (0..tabs.len()).filter(|&i| Some(i) != dragged_idx).chain(dragged_idx).collect();
    for i in order {
        let (t, (rect, resp)) = (&tabs[i], &rects[i]);
        let is_active = i == active;
        // A pair is "on screen" when either half is the active tab; it then gets one card.
        let pair_shown = match t.pair {
            PairPos::None => false,
            PairPos::Left => is_active || i + 1 == active,
            PairPos::Right => is_active || i == active + 1,
        };
        let mut tab_painter = painter.clone();
        tab_painter.multiply_opacity(fade_in[i]);
        let p = &tab_painter;
        let radius = CornerRadius::same(10);
        if t.pair == PairPos::Left && pair_shown {
            let whole = rects.get(i + 1).map_or(*rect, |(r, _)| rect.union(*r));
            p.add(pal.card_shadow().as_shape(whole, radius));
            p.rect_filled(whole, radius, pal.card);
        } else if t.pair == PairPos::None {
            // The active card and the hover highlight cross-fade rather than switching instantly.
            let card = ctx.animate_bool_with_time(Id::unique(("tab-card", t.id)), is_active || resp.dragged(), FADE_TIME as f32);
            let hover = ctx.animate_bool_with_time(Id::unique(("tab-hover", t.id)), resp.hovered(), 0.1);
            if card > 0.0 {
                let mut shadow = pal.card_shadow();
                shadow.color = shadow.color.gamma_multiply(card);
                p.add(shadow.as_shape(*rect, radius));
                p.rect_filled(*rect, radius, pal.card.gamma_multiply(card));
            }
            if hover > 0.0 && card < 1.0 {
                p.rect_filled(*rect, radius, pal.hover.gamma_multiply(hover * (1.0 - card)));
            }
        } else if resp.hovered() && !pair_shown {
            p.rect_filled(*rect, radius, pal.hover);
        }
        if t.pair == PairPos::Right {
            // Thin line between the two halves.
            p.line_segment(
                [egui::pos2(rect.left(), rect.top() + 7.0), egui::pos2(rect.left(), rect.bottom() - 7.0)],
                Stroke::new(1.0, pal.border),
            );
        }

        // Icon: spinner while loading, else favicon, else a soft placeholder dot.
        let icon_c = if t.pinned { rect.center() } else { egui::pos2(rect.left() + 17.0, rect.center().y) };
        let color = if is_active { pal.text } else { pal.muted };
        if t.loading {
            spinner(p, icon_c, 5.5, time, pal.accent);
        } else if let Some(tex) = t.favicon {
            p.image(
                tex.id(),
                Rect::from_center_size(icon_c, Vec2::splat(16.0)),
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            p.circle_filled(icon_c, 5.0, pal.muted.gamma_multiply(0.45));
        }

        let mut on_close = false;
        let mut on_speaker = false;
        if t.pinned {
            // A small dot marks a pinned tab that is playing (accent) or muted (grey).
            if t.audible || t.muted {
                let c = icon_c + Vec2::new(8.0, 7.0);
                p.circle_filled(c, 3.5, if is_active { pal.card } else { pal.bg });
                p.circle_filled(c, 2.5, if t.muted { pal.muted } else { pal.accent });
            }
        } else {
            let show_close = (is_active || resp.hovered()) && tab_w >= 76.0;
            let close_rect = Rect::from_center_size(egui::pos2(rect.right() - 15.0, rect.center().y), Vec2::splat(20.0));
            let show_speaker = (t.audible || t.muted) && tab_w >= 76.0;
            // Fixed beside the close button's spot, so it doesn't slide under the pointer when the
            // close button appears on hover.
            let speaker_x = if tab_w >= 76.0 { close_rect.left() - 11.0 } else { rect.right() - 15.0 };
            let speaker_rect = Rect::from_center_size(egui::pos2(speaker_x, rect.center().y), Vec2::splat(20.0));
            let text_left = rect.left() + 31.0;
            let text_right = if show_speaker {
                speaker_rect.left() - 2.0
            } else if show_close {
                close_rect.left() - 2.0
            } else {
                rect.right() - 8.0
            };
            if text_right > text_left + 4.0 {
                // Fade the title out instead of hard-clipping it.
                let text_rect = Rect::from_min_max(egui::pos2(text_left, rect.top()), egui::pos2(text_right, rect.bottom()));
                let galley = p.layout_no_wrap(t.title.to_string(), FontId::proportional(13.0), color);
                let pos = egui::pos2(text_left, rect.center().y - galley.size().y / 2.0);
                p.with_clip_rect(text_rect).galley(pos, galley.clone(), color);
                if galley.size().x > text_rect.width() {
                    let bg = if is_active || pair_shown {
                        pal.card
                    } else if resp.hovered() {
                        blend(pal.bg, pal.hover)
                    } else {
                        pal.bg
                    };
                    fade_right(p, text_rect, bg);
                }
            }
            let over = |r: Rect| resp.hover_pos().is_some_and(|pos| r.contains(pos));
            if show_speaker {
                if over(speaker_rect) {
                    p.rect_filled(speaker_rect, CornerRadius::same(6), pal.hover);
                }
                let icon = if t.muted { Icon::Muted } else { Icon::Speaker };
                paint_icon(p, speaker_rect, icon, if t.muted { pal.muted } else { pal.accent });
                on_speaker = resp.interact_pointer_pos().is_some_and(|pos| speaker_rect.contains(pos));
            }
            if show_close {
                if over(close_rect) {
                    p.rect_filled(close_rect, CornerRadius::same(6), pal.hover);
                }
                paint_icon(p, close_rect, Icon::Close, if over(close_rect) { pal.text } else { pal.muted });
                on_close = resp.interact_pointer_pos().is_some_and(|pos| close_rect.contains(pos));
            }
        }

        let pointer = ui.input(|inp| inp.pointer.latest_pos());
        let torn = |p: Pos2| (p.y - strip.center().y).abs() > DETACH_DIST;
        if resp.dragged() {
            match pointer {
                // Far enough out: show the tab following the pointer, ready to become a window.
                Some(p) if torn(p) => {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    tab_ghost(ui.ctx(), pal, t, p, tab_w);
                }
                Some(p) => {
                    if let Some(to) = reorder_target(i, p.x, &centers) {
                        event = Some(TabEvent::Move { from: i, to });
                    }
                }
                None => {}
            }
        } else if resp.drag_stopped() && pointer.is_some_and(torn) {
            event = pointer.map(|p| TabEvent::Detach(i, p));
        } else if resp.clicked() {
            event = Some(if on_close {
                TabEvent::Close(i)
            } else if on_speaker {
                TabEvent::Mute(i)
            } else {
                TabEvent::Select(i)
            });
        } else if resp.secondary_clicked() {
            event = Some(TabEvent::Menu(i, rect.left()));
        } else if resp.middle_clicked() && !t.pinned {
            event = Some(TabEvent::Close(i));
        } else if resp.drag_started() {
            event = Some(TabEvent::Select(i));
        }
        if !t.title.is_empty() && !resp.dragged() {
            resp.clone().on_hover_text_at_pointer(t.title);
        }
    }
    (event, strip.right())
}

/// The tab being torn off, following the pointer. It's a tiny borderless always-on-top window
/// (not a paint on our own window), so it stays visible over the page, which is a separate native
/// window stacked above ours. It ignores the mouse and never takes focus, so the drop still lands.
fn tab_ghost(ctx: &egui::Context, pal: &Palette, t: &TabView, pointer: Pos2, tab_w: f32) {
    let origin = ctx.input(|i| i.viewport().inner_rect.map(|r| r.min)).unwrap_or(Pos2::ZERO);
    let size = Vec2::new(tab_w.clamp(180.0, TAB_MAX), TAB_H);
    let at = origin + pointer.to_vec2() + Vec2::new(-24.0, -TAB_H / 2.0);
    let builder = egui::ViewportBuilder::default()
        .with_title(t.title)
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false)
        .with_mouse_passthrough(true)
        .with_position(at)
        .with_inner_size(size);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("tab-ghost"), builder, |ui, _| {
        let p = ui.painter();
        let r = Rect::from_min_size(Pos2::ZERO, size);
        p.rect_filled(r, CornerRadius::ZERO, pal.card);
        p.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, pal.accent), egui::StrokeKind::Inside);
        let ic = egui::pos2(r.left() + 17.0, r.center().y);
        match t.favicon {
            Some(tex) => {
                p.image(tex.id(), Rect::from_center_size(ic, Vec2::splat(16.0)), Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                p.circle_filled(ic, 5.0, pal.muted.gamma_multiply(0.45));
            }
        }
        let text = Rect::from_min_max(egui::pos2(r.left() + 31.0, r.top()), egui::pos2(r.right() - 10.0, r.bottom()));
        p.with_clip_rect(text).text(egui::pos2(text.left(), r.center().y), egui::Align2::LEFT_CENTER, t.title, FontId::proportional(13.0), pal.text);
    });
}

/// `top` (premultiplied, possibly translucent) composited over an opaque `bottom`.
pub fn blend(bottom: Color32, top: Color32) -> Color32 {
    let a = top.a() as f32 / 255.0;
    let mix = |b: u8, t: u8| (b as f32 * (1.0 - a) + t as f32).round().min(255.0) as u8;
    Color32::from_rgb(mix(bottom.r(), top.r()), mix(bottom.g(), top.g()), mix(bottom.b(), top.b()))
}

/// Fade the last 18px of `rect` into `bg`, so long text trails off softly.
fn fade_right(p: &Painter, rect: Rect, bg: Color32) {
    let w = 18.0_f32.min(rect.width());
    let r = Rect::from_min_max(egui::pos2(rect.right() - w, rect.top() + 2.0), egui::pos2(rect.right(), rect.bottom() - 2.0));
    let mut mesh = egui::Mesh::default();
    let clear = bg.gamma_multiply(0.0);
    mesh.colored_vertex(r.left_top(), clear);
    mesh.colored_vertex(r.right_top(), bg);
    mesh.colored_vertex(r.right_bottom(), bg);
    mesh.colored_vertex(r.left_bottom(), clear);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_halves_touch() {
        use PairPos::*;
        // Normal tabs are GAP apart; the two halves of a pair have no gap between them.
        let lefts = tab_lefts(&[100.0, 100.0, 100.0, 100.0], &[None, Left, Right, None]);
        assert_eq!(lefts, vec![0.0, 100.0 + GAP, 200.0 + GAP, 300.0 + 2.0 * GAP]);
    }

    #[test]
    fn reorder_math() {
        let c = [50.0, 150.0, 250.0];
        assert_eq!(reorder_target(0, 120.0, &c), None);
        assert_eq!(reorder_target(0, 151.0, &c), Some(1));
        assert_eq!(reorder_target(1, 49.0, &c), Some(0));
        assert_eq!(reorder_target(1, 260.0, &c), Some(2));
        assert_eq!(reorder_target(2, 400.0, &c), None);
    }

    #[test]
    fn blending() {
        let b = blend(Color32::from_rgb(0, 0, 0), Color32::from_rgba_unmultiplied(255, 255, 255, 128));
        assert!((b.r() as i32 - 128).abs() <= 1);
    }
}
