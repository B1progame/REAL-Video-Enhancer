use eframe::egui::{self, Color32, FontId, Stroke, TextStyle, Vec2};

pub const ACCENTS: [(&str, [u8; 3]); 5] = [
    ("Lagoon", [0, 190, 190]),
    ("Glacier", [64, 157, 219]),
    ("Cobalt", [83, 112, 220]),
    ("Aurora", [142, 104, 209]),
    ("Mint", [38, 157, 119]),
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
    pub const CARD: u8 = 12;
    pub const DIALOG: u8 = 16;
}

pub mod type_scale {
    pub const PAGE: f32 = 27.0;
    pub const SECTION: f32 = 16.0;
    pub const BODY: f32 = 15.0;
    pub const BUTTON: f32 = 14.0;
    pub const LABEL: f32 = 13.0;
    pub const CAPTION: f32 = 12.0;
    pub const MONOSPACE: f32 = 12.0;
}

pub mod stroke {
    pub const BORDER: f32 = 1.0;
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
            canvas: Color32::from_rgb(242, 245, 247),
            sidebar: Color32::from_rgb(248, 249, 250),
            surface: Color32::WHITE,
            elevated: Color32::from_rgb(234, 239, 242),
            border: Color32::from_rgb(221, 227, 231),
            text: Color32::from_rgb(26, 32, 38),
            secondary: Color32::from_rgb(82, 94, 104),
            muted: Color32::from_rgb(116, 127, 136),
            accent,
            success: Color32::from_rgb(35, 137, 100),
            warning: Color32::from_rgb(166, 111, 35),
            error: Color32::from_rgb(190, 69, 73),
        }
    } else {
        Palette {
            canvas: Color32::from_rgb(14, 17, 21),
            sidebar: Color32::from_rgb(18, 22, 27),
            surface: Color32::from_rgb(25, 30, 36),
            elevated: Color32::from_rgb(32, 39, 47),
            border: Color32::from_rgb(47, 56, 66),
            text: Color32::from_rgb(242, 245, 247),
            secondary: Color32::from_rgb(166, 177, 186),
            muted: Color32::from_rgb(117, 131, 142),
            accent,
            success: Color32::from_rgb(82, 199, 154),
            warning: Color32::from_rgb(232, 184, 109),
            error: Color32::from_rgb(240, 120, 120),
        }
    }
}

pub fn apply(ctx: &egui::Context, accent: Color32, appearance: &str, reduce_motion: bool) {
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
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent.gamma_multiply(0.52));
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(radius::CONTROL);
    visuals.widgets.inactive.bg_fill = palette.surface;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(radius::CONTROL);
    visuals.widgets.noninteractive.bg_fill = palette.surface;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke.color = palette.text;
    visuals.hyperlink_color = palette.accent;
    visuals.window_corner_radius = egui::CornerRadius::same(radius::DIALOG);
    visuals.menu_corner_radius = egui::CornerRadius::same(radius::CONTROL);

    ctx.set_theme(if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    });
    ctx.global_style_mut(|style| {
        style.visuals = visuals;
        style.spacing.item_spacing = Vec2::new(space::MD, 10.0);
        style.spacing.button_padding = Vec2::new(14.0, 9.0);
        style.spacing.interact_size = Vec2::new(40.0, 40.0);
        style.spacing.icon_width = 18.0;
        style.spacing.icon_width_inner = 12.0;
        style.animation_time = if reduce_motion {
            0.0
        } else {
            motion::NAV_SECONDS
        };
        style.explanation_tooltips = true;
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(type_scale::PAGE));
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
