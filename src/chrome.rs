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
const STROKE: f32 = 1.6;

#[derive(Clone, Copy)]
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
}

/// Draw `icon` centred in `rect` (designed on a 16×16 grid).
pub fn paint_icon(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = Stroke::new(STROKE, color);
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(pts.iter().map(|&(x, y)| c + Vec2::new(x, y)).collect(), s));
    };
    match icon {
        Icon::Back => {
            line(&[(5.5, 0.0), (-5.5, 0.0)]);
            line(&[(-1.0, -4.5), (-5.5, 0.0), (-1.0, 4.5)]);
        }
        Icon::Forward => {
            line(&[(-5.5, 0.0), (5.5, 0.0)]);
            line(&[(1.0, -4.5), (5.5, 0.0), (1.0, 4.5)]);
        }
        Icon::Reload => {
            let r = 5.5;
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = -1.1 + i as f32 / 24.0 * 5.0; // ~286° of arc
                    c + Vec2::new(a.cos(), a.sin()) * r
                })
                .collect();
            let end = *pts.last().unwrap();
            p.add(Shape::line(pts, s));
            // Arrow head at the arc's end, pointing along the direction of travel.
            p.add(Shape::line(vec![end + Vec2::new(-3.4, -0.6), end, end + Vec2::new(0.4, -3.4)], s));
        }
        Icon::Stop | Icon::Close => {
            let k = if matches!(icon, Icon::Stop) { 4.5 } else { 3.8 };
            line(&[(-k, -k), (k, k)]);
            line(&[(-k, k), (k, -k)]);
        }
        Icon::Plus => {
            line(&[(-5.5, 0.0), (5.5, 0.0)]);
            line(&[(0.0, -5.5), (0.0, 5.5)]);
        }
        Icon::Lock => {
            let body = Rect::from_center_size(c + Vec2::new(0.0, 1.8), Vec2::new(8.0, 6.0));
            p.rect_filled(body, CornerRadius::same(1), color);
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0);
                    c + Vec2::new(0.0, -1.4) + Vec2::new(a.cos() * 2.6, a.sin() * 3.0)
                })
                .collect();
            let (first, last) = (pts[0], *pts.last().unwrap());
            p.add(Shape::line(pts, Stroke::new(1.4, color)));
            p.line_segment([first, first + Vec2::new(0.0, 1.6)], Stroke::new(1.4, color));
            p.line_segment([last, last + Vec2::new(0.0, 1.6)], Stroke::new(1.4, color));
        }
        Icon::Search => {
            p.circle_stroke(c + Vec2::new(-1.2, -1.2), 4.2, Stroke::new(1.5, color));
            line(&[(1.9, 1.9), (5.0, 5.0)]);
        }
        Icon::Up => line(&[(-4.5, 2.2), (0.0, -2.3), (4.5, 2.2)]),
        Icon::Down => line(&[(-4.5, -2.2), (0.0, 2.3), (4.5, -2.2)]),
        Icon::Download => {
            line(&[(0.0, -5.5), (0.0, 3.0)]);
            line(&[(-3.8, -0.8), (0.0, 3.0), (3.8, -0.8)]);
            line(&[(-5.5, 5.5), (5.5, 5.5)]);
        }
        Icon::Puzzle => {
            let body = Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::splat(11.0));
            p.rect_stroke(body, CornerRadius::same(2), s, egui::StrokeKind::Inside);
            p.circle_filled(egui::pos2(body.center().x, body.top() - 1.3), 2.5, color);
            p.circle_filled(egui::pos2(body.right() + 1.3, body.center().y), 2.5, color);
        }
        Icon::Shield | Icon::ShieldOff => {
            let outline = [(0.0, -6.5), (5.5, -4.3), (5.0, 1.2), (0.0, 6.5), (-5.0, 1.2), (-5.5, -4.3), (0.0, -6.5)];
            line(&outline);
            if matches!(icon, Icon::Shield) {
                line(&[(-2.3, 0.0), (-0.5, 1.9), (2.6, -1.6)]);
            } else {
                line(&[(-6.5, 6.5), (6.5, -6.5)]);
            }
        }
        Icon::Speaker | Icon::Muted => {
            let body = [(-6.0, -2.0), (-3.5, -2.0), (0.5, -5.5), (0.5, 5.5), (-3.5, 2.0), (-6.0, 2.0)];
            p.add(Shape::convex_polygon(body.iter().map(|&(x, y)| c + Vec2::new(x, y)).collect(), color, Stroke::NONE));
            if matches!(icon, Icon::Muted) {
                line(&[(3.0, -2.5), (7.0, 2.5)]);
                line(&[(3.0, 2.5), (7.0, -2.5)]);
            } else {
                line(&[(3.0, -2.5), (4.3, 0.0), (3.0, 2.5)]);
                line(&[(5.2, -4.8), (7.2, 0.0), (5.2, 4.8)]);
            }
        }
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
    let c = rect.center();
    let pts: Vec<Pos2> = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { 7.0 } else { 3.0 };
            let a = std::f32::consts::PI * (i as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
            c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    let p = ui.painter();
    if filled {
        // A star is star-shaped around its centre, so a triangle fan fills it.
        for i in 0..10 {
            p.add(Shape::convex_polygon(vec![c, pts[i], pts[(i + 1) % 10]], pal.accent, Stroke::NONE));
        }
    } else {
        p.add(Shape::closed_line(pts, Stroke::new(1.4, pal.muted)));
    }
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
    let s = Stroke::new(1.0, color);
    let c = rect.center();
    match kind {
        WinBtn::Minimize => {
            p.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], s);
        }
        WinBtn::Maximize => {
            p.rect_stroke(Rect::from_center_size(c, Vec2::splat(10.0)), CornerRadius::same(1), s, egui::StrokeKind::Inside);
        }
        WinBtn::Restore => {
            p.rect_stroke(Rect::from_center_size(c + Vec2::new(-1.0, 1.0), Vec2::splat(8.0)), CornerRadius::same(1), s, egui::StrokeKind::Inside);
            p.add(Shape::line(
                vec![c + Vec2::new(-3.0, -3.0), c + Vec2::new(-3.0, -5.0), c + Vec2::new(5.0, -5.0), c + Vec2::new(5.0, 3.0), c + Vec2::new(3.0, 3.0)],
                s,
            ));
        }
        WinBtn::Close => {
            p.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], s);
            p.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], s);
        }
    }
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
}

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
    let mut rects = Vec::with_capacity(tabs.len());
    for (i, t) in tabs.iter().enumerate() {
        let rect = Rect::from_min_size(egui::pos2(strip.left() + lefts[i] - off, strip.top()), Vec2::new(widths[i], TAB_H));
        // Stable per-tab id so a drag survives the tab moving to a new slot.
        // Halves of a split pair can't be dragged.
        let sense = if t.pair == PairPos::None { Sense::click_and_drag() } else { Sense::click() };
        let resp = ui.interact(rect.intersect(strip), Id::unique(("tab", t.id)), sense);
        rects.push((rect, resp));
    }
    let centers: Vec<f32> = rects.iter().map(|(r, _)| r.center().x).collect();
    for (i, (t, (rect, resp))) in tabs.iter().zip(&rects).enumerate() {
        let is_active = i == active;
        // A pair is "on screen" when either half is the active tab; it then gets one card.
        let pair_shown = match t.pair {
            PairPos::None => false,
            PairPos::Left => is_active || i + 1 == active,
            PairPos::Right => is_active || i == active + 1,
        };
        let p = &painter;
        let radius = CornerRadius::same(10);
        if t.pair == PairPos::Left && pair_shown {
            let whole = rects.get(i + 1).map_or(*rect, |(r, _)| rect.union(*r));
            p.add(pal.card_shadow().as_shape(whole, radius));
            p.rect_filled(whole, radius, pal.card);
        } else if (is_active || resp.dragged()) && t.pair == PairPos::None {
            p.add(pal.card_shadow().as_shape(*rect, radius));
            p.rect_filled(*rect, radius, pal.card);
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

        if resp.dragged() {
            if let Some(x) = resp.interact_pointer_pos().map(|p| p.x) {
                if let Some(to) = reorder_target(i, x, &centers) {
                    event = Some(TabEvent::Move { from: i, to });
                }
            }
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
