use eframe::egui::{self, Color32, FontId, Stroke, TextStyle, Vec2};

pub const ACCENTS: [(&str, [u8; 3]); 5] = [
    ("Lagoon", [66, 204, 205]),
    ("Glacier", [93, 182, 230]),
    ("Cobalt", [93, 133, 238]),
    ("Aurora", [159, 128, 232]),
    ("Mint", [79, 190, 158]),
];

pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

pub mod radius {
    pub const CONTROL: u8 = 8;
    pub const CARD: u8 = 14;
    pub const DIALOG: u8 = 16;
}

pub mod motion {
    pub const HOVER_SECONDS: f32 = 0.12;
    pub const NAV_SECONDS: f32 = 0.22;
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub canvas: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub elevated: Color32,
    pub border: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
}

pub fn palette(light: bool, accent: Color32) -> Palette {
    let accent = accent_for_theme(accent, light);
    if light {
        Palette {
            canvas: Color32::from_rgb(244, 247, 249),
            sidebar: Color32::from_rgb(249, 250, 251),
            surface: Color32::WHITE,
            elevated: Color32::from_rgb(235, 240, 243),
            border: Color32::from_rgb(219, 226, 231),
            text: Color32::from_rgb(25, 34, 42),
            secondary: Color32::from_rgb(85, 102, 114),
            muted: Color32::from_rgb(116, 132, 143),
            accent,
            success: Color32::from_rgb(35, 137, 100),
            warning: Color32::from_rgb(166, 111, 35),
            error: Color32::from_rgb(190, 69, 73),
        }
    } else {
        Palette {
            canvas: Color32::from_rgb(11, 15, 20),
            sidebar: Color32::from_rgb(16, 21, 27),
            surface: Color32::from_rgb(21, 28, 36),
            elevated: Color32::from_rgb(27, 37, 48),
            border: Color32::from_rgb(41, 53, 64),
            text: Color32::from_rgb(244, 247, 250),
            secondary: Color32::from_rgb(156, 170, 183),
            muted: Color32::from_rgb(104, 122, 137),
            accent,
            success: Color32::from_rgb(82, 199, 154),
            warning: Color32::from_rgb(232, 184, 109),
            error: Color32::from_rgb(240, 120, 120),
        }
    }
}

pub fn apply(ctx: &egui::Context, accent: Color32, appearance: &str) {
    let light = appearance == "light";
    let palette = palette(light, accent);
    let mut visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.window_fill = palette.sidebar;
    visuals.panel_fill = palette.sidebar;
    visuals.extreme_bg_color = palette.canvas;
    visuals.faint_bg_color = palette.elevated;
    visuals.selection.bg_fill = palette.accent.gamma_multiply(0.28);
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.fg_stroke.color = contrast_text(palette.accent);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(radius::CONTROL);
    visuals.widgets.hovered.bg_fill = palette.accent.gamma_multiply(0.15);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent.gamma_multiply(0.58));
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(radius::CONTROL);
    visuals.widgets.inactive.bg_fill = palette.elevated;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(radius::CONTROL);
    visuals.widgets.noninteractive.bg_fill = palette.surface;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke.color = palette.text;
    visuals.window_corner_radius = egui::CornerRadius::same(radius::DIALOG);
    visuals.menu_corner_radius = egui::CornerRadius::same(radius::CONTROL);
    visuals.popup_shadow = egui::epaint::Shadow::NONE;

    ctx.set_theme(if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    });
    ctx.global_style_mut(|style| {
        style.visuals = visuals;
        style.spacing.item_spacing = Vec2::new(space::MD, space::SM);
        style.spacing.button_padding = Vec2::new(space::LG, 10.0);
        style.spacing.interact_size = Vec2::new(40.0, 38.0);
        style.spacing.icon_width = 18.0;
        style.spacing.icon_width_inner = 12.0;
        style.animation_time = motion::NAV_SECONDS;
        style.explanation_tooltips = true;
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(29.0));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(12.0));
    });
}

pub fn accent_for_theme(accent: Color32, light: bool) -> Color32 {
    accent.gamma_multiply(if light { 0.78 } else { 1.0 })
}

pub fn border_color(light: bool) -> Color32 {
    if light {
        Color32::from_rgb(219, 226, 231)
    } else {
        Color32::from_rgb(41, 53, 64)
    }
}

pub fn secondary_text_color(light: bool) -> Color32 {
    palette(light, Color32::WHITE).secondary
}

pub fn contrast_text(background: Color32) -> Color32 {
    let [red, green, blue, _] = background.to_array();
    if (u32::from(red) * 299 + u32::from(green) * 587 + u32::from(blue) * 114) / 1000 > 145 {
        Color32::from_rgb(15, 22, 28)
    } else {
        Color32::WHITE
    }
}
