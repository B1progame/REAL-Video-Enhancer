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
    primary_button_enabled(ui, label, true, 210.0)
}

pub fn toggle(
    ui: &mut Ui,
    value: &mut bool,
    label: &str,
    accent: Color32,
    reduce_motion: bool,
) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(38.0, 22.0), egui::Sense::click());
    let keyboard_toggle =
        response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Space));
    if response.clicked() || keyboard_toggle {
        *value = !*value;
        response.mark_changed();
    }
    let progress = ui.ctx().animate_bool_with_time(
        response.id,
        *value,
        if reduce_motion {
            0.0
        } else {
            theme::motion::HOVER_SECONDS
        },
    );
    let palette = theme::palette(!ui.visuals().dark_mode, accent);
    let track = if *value {
        if response.hovered() {
            accent.gamma_multiply(0.88)
        } else {
            accent
        }
    } else {
        palette.muted.gamma_multiply(0.45)
    };
    ui.painter().rect_filled(rect, 11.0, track);
    ui.painter().rect_stroke(
        rect,
        11.0,
        if response.has_focus() {
            Stroke::new(1.5, accent)
        } else {
            Stroke::NONE
        },
        egui::StrokeKind::Outside,
    );
    let inset = 2.0;
    let left = rect.left() + inset + 9.0;
    let right = rect.right() - inset - 9.0;
    let knob_x = egui::lerp(left..=right, progress);
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), 8.0, Color32::WHITE);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *value, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn metadata_chip(ui: &mut Ui, text: &str, accent: Color32) {
    let palette = theme::palette(!ui.visuals().dark_mode, accent);
    egui::Frame::new()
        .fill(palette.elevated)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(9, 5))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(11.0)
                    .strong()
                    .color(palette.secondary),
            );
        });
}

pub fn copyable_path(ui: &mut Ui, path: &str, placeholder: &str) {
    let palette = theme::palette(!ui.visuals().dark_mode, ui.visuals().selection.stroke.color);
    egui::Frame::new()
        .fill(palette.elevated)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let label = if path.is_empty() { placeholder } else { path };
                let width = if path.is_empty() {
                    ui.available_width()
                } else {
                    (ui.available_width() - 66.0).max(120.0)
                };
                let response = ui.add_sized(
                    Vec2::new(width, 30.0),
                    egui::Label::new(
                        RichText::new(label)
                            .monospace()
                            .size(12.0)
                            .color(palette.secondary),
                    )
                    .truncate(),
                );
                if !path.is_empty() {
                    response.on_hover_text(path);
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(path.to_owned());
                    }
                }
            });
        });
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
