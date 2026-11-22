use eframe::egui::{self, Color32, RichText, Stroke, Ui, Vec2};

use super::theme;

pub fn card<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    let palette = theme::palette(!ui.visuals().dark_mode, ui.visuals().selection.stroke.color);
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(theme::radius::CARD)
        .inner_margin(egui::Margin::same(20))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(title).size(16.0).strong().color(palette.text));
            ui.add_space(theme::space::SM);
            content(ui)
        })
        .inner
}

pub fn primary_button(ui: &mut Ui, label: impl Into<RichText>) -> egui::Response {
    primary_button_enabled(ui, label, true, 260.0)
}

pub fn primary_button_enabled(
    ui: &mut Ui,
    label: impl Into<RichText>,
    enabled: bool,
    width: f32,
) -> egui::Response {
    let accent = ui.visuals().selection.stroke.color;
    let text = theme::contrast_text(accent);
    ui.add_enabled(
        enabled,
        egui::Button::new(label.into().strong().color(text))
            .fill(accent)
            .stroke(Stroke::NONE)
            .min_size(Vec2::new(width, 44.0)),
    )
}

pub fn status_dot(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
        ui.painter().circle_filled(dot.center(), 3.5, color);
        ui.add_space(theme::space::XS);
        ui.label(label);
    });
}
