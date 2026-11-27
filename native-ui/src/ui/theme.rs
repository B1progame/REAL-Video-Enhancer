//! Design tokens for the glass interface.
//!
//! Surfaces are translucent so the Windows backdrop (acrylic or mica) shows
//! through. How translucent is a user setting, held here so that every call to
//! [`palette`] agrees without threading it through each widget.

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle, Vec2};

/// Alpha of the window tint over the backdrop. 255 means fully solid.
static TINT_ALPHA: AtomicU8 = AtomicU8::new(255);
static CORNER_RADIUS: AtomicU8 = AtomicU8::new(14);

/// Name, primary colour, gradient partner.
pub const ACCENTS: [(&str, [u8; 3], [u8; 3]); 10] = [
    ("Lagoon", [0, 190, 190], [64, 132, 246]),
    ("Aurora", [150, 108, 244], [45, 200, 214]),
    ("Cobalt", [86, 116, 246], [152, 96, 240]),
    ("Glacier", [56, 168, 232], [120, 220, 232]),
    ("Mint", [44, 190, 134], [150, 214, 92]),
    ("Ember", [246, 118, 62], [236, 68, 128]),
    ("Rose", [236, 84, 140], [160, 100, 238]),
    ("Gold", [228, 170, 48], [242, 112, 72]),
    ("Crimson", [226, 68, 84], [246, 140, 66]),
    ("Graphite", [150, 164, 184], [104, 116, 140]),
];

pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 18.0;
}

pub mod type_scale {
    pub const PAGE: f32 = 28.0;
    pub const SECTION: f32 = 15.5;
    pub const BODY: f32 = 14.0;
    pub const BUTTON: f32 = 13.5;
    pub const CAPTION: f32 = 12.0;
    pub const MONOSPACE: f32 = 12.0;
}

pub mod motion {
    pub const HOVER_SECONDS: f32 = 0.14;
    pub const NAV_SECONDS: f32 = 0.22;
    pub const PAGE_SECONDS: f32 = 0.30;
}

pub fn set_tint_alpha(alpha: u8) {
    TINT_ALPHA.store(alpha, Ordering::Relaxed);
}

pub fn set_radius(radius: u8) {
    CORNER_RADIUS.store(radius.min(24), Ordering::Relaxed);
}

pub fn radius() -> u8 {
    CORNER_RADIUS.load(Ordering::Relaxed)
}

pub fn control_radius() -> u8 {
    (radius() * 2 / 3).max(2)
}

#[derive(Clone, Copy)]
pub struct Palette {
    /// Window tint painted over the backdrop.
    pub canvas: Color32,
    pub surface: Color32,
    pub elevated: Color32,
    /// Opaque, for menus and dialogs that must stay readable.
    pub popup: Color32,
    pub border: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_alt: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
}

fn alpha(red: u8, green: u8, blue: u8, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

pub fn palette(light: bool, accent: Color32) -> Palette {
    let tint = TINT_ALPHA.load(Ordering::Relaxed);
    let accent_alt = accent_for_theme(partner(accent), light);
    let accent = accent_for_theme(accent, light);
    if light {
        Palette {
            canvas: alpha(241, 243, 248, tint),
            surface: alpha(255, 255, 255, 176),
            elevated: alpha(18, 28, 56, 16),
            popup: Color32::from_rgb(250, 251, 253),
            border: alpha(18, 28, 56, 30),
            text: Color32::from_rgb(18, 22, 32),
            secondary: Color32::from_rgb(62, 70, 88),
            muted: Color32::from_rgb(98, 107, 126),
            accent,
            accent_alt,
            success: Color32::from_rgb(20, 128, 88),
            warning: Color32::from_rgb(154, 98, 12),
            error: Color32::from_rgb(188, 52, 64),
        }
    } else {
        Palette {
            canvas: alpha(11, 13, 19, tint),
            surface: alpha(255, 255, 255, 12),
            elevated: alpha(255, 255, 255, 20),
            popup: Color32::from_rgb(24, 27, 36),
            border: alpha(255, 255, 255, 24),
            text: Color32::from_rgb(244, 246, 251),
            secondary: Color32::from_rgb(190, 196, 212),
            muted: Color32::from_rgb(142, 150, 170),
            accent,
            accent_alt,
            success: Color32::from_rgb(78, 214, 160),
            warning: Color32::from_rgb(242, 192, 108),
            error: Color32::from_rgb(250, 118, 128),
        }
    }
}

/// Darken the accent on light surfaces. Alpha stays opaque.
pub fn accent_for_theme(accent: Color32, light: bool) -> Color32 {
    if !light {
        return accent;
    }
    let scale = |channel: u8| (f32::from(channel) * 0.80) as u8;
    Color32::from_rgb(scale(accent.r()), scale(accent.g()), scale(accent.b()))
}

/// The second gradient stop: the preset's partner, or a hue-rotated custom one.
pub fn partner(accent: Color32) -> Color32 {
    for (_, primary, second) in ACCENTS {
        let preset = Color32::from_rgb(primary[0], primary[1], primary[2]);
        if preset == accent || accent_for_theme(preset, true) == accent {
            return Color32::from_rgb(second[0], second[1], second[2]);
        }
    }
    Color32::from_rgb(accent.b(), accent.r(), accent.g())
}

pub fn contrast_text(background: Color32) -> Color32 {
    let [red, green, blue, _] = background.to_array();
    if (u32::from(red) * 299 + u32::from(green) * 587 + u32::from(blue) * 114) / 1000 > 145 {
        Color32::from_rgb(12, 16, 24)
    } else {
        Color32::WHITE
    }
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn title_font(size: f32) -> FontId {
    FontId::new(size, semibold())
}

/// Use the Windows system faces when present; egui's built-in fonts stay as
/// fallbacks for symbols.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let windows = std::env::var_os("WINDIR").map(std::path::PathBuf::from);
    let mut load = |name: &str, file: &str| -> bool {
        let Some(root) = windows.as_deref() else {
            return false;
        };
        let Ok(bytes) = std::fs::read(Path::new(root).join("Fonts").join(file)) else {
            return false;
        };
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        true
    };
    let regular = load("ui-regular", "segoeui.ttf");
    let bold = load("ui-semibold", "seguisb.ttf");
    if regular {
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "ui-regular".to_owned());
    }
    let mut heavy = fallback;
    if bold {
        heavy.insert(0, "ui-semibold".to_owned());
    }
    fonts.families.insert(semibold(), heavy);
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, accent: Color32, appearance: &str, reduce_motion: bool) {
    let light = appearance == "light";
    let palette = palette(light, accent);
    let corner = egui::CornerRadius::same(control_radius());
    let mut visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.window_fill = palette.popup;
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.panel_fill = Color32::TRANSPARENT;
    visuals.extreme_bg_color = if light {
        alpha(255, 255, 255, 220)
    } else {
        alpha(0, 0, 0, 90)
    };
    visuals.faint_bg_color = palette.elevated;
    visuals.override_text_color = None;
    visuals.selection.bg_fill = palette.accent.gamma_multiply(0.30);
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.hyperlink_color = palette.accent;
    visuals.slider_trailing_fill = true;
    visuals.window_corner_radius = egui::CornerRadius::same(radius());
    visuals.menu_corner_radius = corner;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = palette.surface;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    widgets.noninteractive.fg_stroke.color = palette.text;
    for state in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        state.corner_radius = corner;
        state.fg_stroke.color = palette.text;
    }
    widgets.inactive.bg_fill = palette.elevated;
    widgets.inactive.weak_bg_fill = palette.elevated;
    widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    widgets.hovered.bg_fill = palette.accent.gamma_multiply(0.20);
    widgets.hovered.weak_bg_fill = palette.accent.gamma_multiply(0.20);
    widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent.gamma_multiply(0.60));
    widgets.hovered.expansion = 1.0;
    widgets.active.bg_fill = palette.accent.gamma_multiply(0.40);
    widgets.active.weak_bg_fill = palette.accent.gamma_multiply(0.40);
    widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    widgets.open.bg_fill = palette.elevated;
    widgets.open.weak_bg_fill = palette.elevated;
    widgets.open.bg_stroke = Stroke::new(1.0, palette.accent.gamma_multiply(0.5));

    ctx.set_theme(if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    });
    ctx.global_style_mut(|style| {
        style.visuals = visuals;
        style.spacing.item_spacing = Vec2::new(10.0, 9.0);
        style.spacing.button_padding = Vec2::new(14.0, 8.0);
        style.spacing.interact_size = Vec2::new(40.0, 34.0);
        style.spacing.icon_width = 18.0;
        style.spacing.icon_width_inner = 11.0;
        style.animation_time = if reduce_motion {
            0.0
        } else {
            motion::NAV_SECONDS
        };
        style.explanation_tooltips = true;
        style
            .text_styles
            .insert(TextStyle::Heading, title_font(type_scale::PAGE));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(type_scale::BODY));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(type_scale::BUTTON));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(type_scale::CAPTION));
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::monospace(type_scale::MONOSPACE),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::{ACCENTS, accent_for_theme, partner};
    use eframe::egui::Color32;

    #[test]
    fn light_theme_accent_stays_opaque() {
        let accent = Color32::from_rgb(0, 190, 190);
        assert_eq!(accent_for_theme(accent, true).a(), 255);
    }

    #[test]
    fn presets_keep_their_gradient_partner_in_both_themes() {
        let (_, primary, second) = ACCENTS[1];
        let accent = Color32::from_rgb(primary[0], primary[1], primary[2]);
        let expected = Color32::from_rgb(second[0], second[1], second[2]);
        assert_eq!(partner(accent), expected);
        assert_eq!(partner(accent_for_theme(accent, true)), expected);
    }
}
