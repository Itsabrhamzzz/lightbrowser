//! One palette for the whole browser: the egui toolbar and the built-in pages (served as /theme.css).

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke};

pub const DEFAULT_ACCENT: &str = "#5b7cfa";

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Window / toolbar background.
    pub bg: Color32,
    /// Raised surfaces: active tab, address pill, find bar.
    pub card: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub shadow: Color32,
}

impl Palette {
    pub fn new(dark: bool, accent: Color32) -> Self {
        if dark {
            Self {
                dark,
                bg: Color32::from_rgb(24, 25, 29),
                card: Color32::from_rgb(40, 41, 48),
                hover: Color32::from_white_alpha(14),
                border: Color32::from_white_alpha(18),
                text: Color32::from_rgb(236, 236, 241),
                muted: Color32::from_rgb(146, 148, 160),
                accent,
                shadow: Color32::from_black_alpha(110),
            }
        } else {
            Self {
                dark,
                bg: Color32::from_rgb(233, 235, 240),
                card: Color32::WHITE,
                hover: Color32::from_black_alpha(14),
                border: Color32::from_black_alpha(16),
                text: Color32::from_rgb(28, 29, 34),
                muted: Color32::from_rgb(106, 110, 122),
                accent,
                shadow: Color32::from_black_alpha(34),
            }
        }
    }

    pub fn card_shadow(&self) -> Shadow {
        Shadow { offset: [0, 1], blur: 8, spread: 0, color: self.shadow }
    }

    /// Install the dark and light palettes as egui's two themes, so egui's own widgets
    /// (text fields, tooltips, selection) match, and switching Windows' theme just works.
    pub fn install(ctx: &egui::Context, accent: Color32) {
        Self::new(true, accent).apply(ctx, egui::Theme::Dark);
        Self::new(false, accent).apply(ctx, egui::Theme::Light);
    }

    fn apply(&self, ctx: &egui::Context, theme: egui::Theme) {
        let p = *self;
        ctx.style_mut_of(theme, |s| {
            let v = &mut s.visuals;
            v.dark_mode = p.dark;
            v.panel_fill = p.bg;
            v.window_fill = p.card;
            v.extreme_bg_color = p.card;
            v.faint_bg_color = p.bg;
            v.override_text_color = None;
            v.hyperlink_color = p.accent;
            v.selection.bg_fill = p.accent.gamma_multiply(0.45);
            v.selection.stroke = Stroke::new(1.0, p.accent);
            v.text_cursor.stroke = Stroke::new(2.0, p.accent);
            v.window_corner_radius = CornerRadius::same(10);
            v.window_stroke = Stroke::new(1.0, p.border);
            v.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: p.shadow };
            for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
                w.corner_radius = CornerRadius::same(8);
                w.bg_stroke = Stroke::NONE;
            }
            v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
            v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
            v.widgets.inactive.weak_bg_fill = p.card;
            v.widgets.hovered.weak_bg_fill = p.hover;
            v.widgets.active.weak_bg_fill = p.hover;
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
            s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        });
    }
}

/// `#rrggbb` → colour (anything else is rejected, which also keeps it safe to put in CSS).
pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

pub fn accent_or_default(s: &str) -> Color32 {
    parse_hex(s).or_else(|| parse_hex(DEFAULT_ACCENT)).unwrap()
}

/// CSS custom properties shared by the start page and extensions page.
pub fn css(accent: &str) -> String {
    let accent = if parse_hex(accent).is_some() { accent } else { DEFAULT_ACCENT };
    format!(
        r#":root {{
  --accent: {accent};
  --accent-soft: color-mix(in srgb, var(--accent) 18%, transparent);
  --bg: #18191d; --bg-2: #1f2026;
  --card: rgba(46, 47, 55, .62); --card-solid: #28292f; --hover: rgba(255,255,255,.07);
  --border: rgba(255,255,255,.08); --text: #ececf1; --muted: #9294a0;
  --shadow: 0 10px 32px rgba(0,0,0,.38), 0 1px 0 rgba(255,255,255,.04) inset;
  --radius: 18px; --radius-sm: 12px;
  --font: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  color-scheme: dark;
}}
@media (prefers-color-scheme: light) {{
  :root {{
    --bg: #e9ebf0; --bg-2: #f3f4f8;
    --card: rgba(255,255,255,.72); --card-solid: #ffffff; --hover: rgba(0,0,0,.05);
    --border: rgba(0,0,0,.07); --text: #1c1d22; --muted: #6a6e7a;
    --shadow: 0 10px 30px rgba(30,35,60,.10), 0 1px 0 rgba(255,255,255,.8) inset;
    color-scheme: light;
  }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#5b7cfa"), Some(Color32::from_rgb(0x5b, 0x7c, 0xfa)));
        assert_eq!(parse_hex("5b7cfa"), None);
        assert_eq!(parse_hex("#5b7cf"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
        assert!(css("red;}body{display:none").contains(DEFAULT_ACCENT));
    }
}
