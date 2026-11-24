use eframe::egui::{self, Color32, Id, RichText, Stroke, Ui, Vec2};

use super::theme;

pub fn card<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    let palette = theme::palette(!ui.visuals().dark_mode, ui.visuals().selection.stroke.color);
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(theme::radius::CARD)
        .inner_margin(egui::Margin::same(22))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new(title)
                    .size(theme::type_scale::SECTION)
                    .strong()
                    .color(palette.text),
            );
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
            let available = ui.available_width().max(1.0);
            ui.set_min_width(available);
            let label = if path.is_empty() { placeholder } else { path };
            let path_label = || {
                egui::Label::new(
                    RichText::new(label)
                        .monospace()
                        .size(theme::type_scale::MONOSPACE)
                        .color(palette.secondary),
                )
                .truncate()
            };
            if !path.is_empty() && available < 220.0 {
                ui.vertical(|ui| {
                    ui.add_sized(Vec2::new(available, 30.0), path_label())
                        .on_hover_text(path);
                    if ui.small_button("Copy path").clicked() {
                        ui.ctx().copy_text(path.to_owned());
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    let text_width = if path.is_empty() {
                        available
                    } else {
                        (available - 66.0).max(1.0)
                    };
                    let response = ui.add_sized(Vec2::new(text_width, 30.0), path_label());
                    if !path.is_empty() {
                        response.on_hover_text(path);
                        if ui.small_button("Copy").clicked() {
                            ui.ctx().copy_text(path.to_owned());
                        }
                    }
                });
            }
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
    let width = width.min(ui.available_width().max(1.0));
    ui.add_enabled(
        enabled,
        egui::Button::new(label.into().strong().color(text))
            .fill(accent)
            .stroke(Stroke::NONE)
            .min_size(Vec2::new(width, 44.0)),
    )
}

pub fn searchable_select(
    ui: &mut Ui,
    id: Id,
    selected: &mut String,
    search: &mut String,
    options: &[String],
    installed: impl Fn(&str) -> bool,
) -> bool {
    let picker_width = ui.available_width().clamp(1.0, 360.0);
    let label = if selected.is_empty() {
        "Choose a model"
    } else {
        selected.as_str()
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .width(picker_width)
        .show_ui(ui, |ui| {
            ui.add_sized(
                Vec2::new(ui.available_width(), 38.0),
                egui::TextEdit::singleline(search)
                    .hint_text("Search models…")
                    .margin(Vec2::new(10.0, 8.0)),
            );
            ui.separator();
            let query = search.trim().to_lowercase();
            let visible = options
                .iter()
                .filter(|option| model_option_matches(option, &query))
                .collect::<Vec<_>>();
            egui::ScrollArea::vertical()
                .id_salt(id.with("results"))
                .max_height(264.0)
                .show(ui, |ui| {
                    if visible.is_empty() {
                        ui.label(
                            RichText::new("No models match this search.")
                                .size(theme::type_scale::CAPTION)
                                .color(
                                    theme::palette(
                                        !ui.visuals().dark_mode,
                                        ui.visuals().selection.stroke.color,
                                    )
                                    .secondary,
                                ),
                        );
                    }
                    for option in visible {
                        let state = if installed(option) {
                            "Installed"
                        } else {
                            "Get in Downloads"
                        };
                        let response = ui
                            .selectable_label(selected == option, format!("{option}  ·  {state}"));
                        if response.clicked() {
                            selected.clone_from(option);
                            changed = true;
                            ui.close();
                        }
                    }
                });
        });
    changed
}

pub fn status_badge(ui: &mut Ui, label: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(if ui.visuals().dark_mode { 0.13 } else { 0.10 }))
        .stroke(Stroke::new(
            theme::stroke::BORDER,
            color.gamma_multiply(0.35),
        ))
        .corner_radius(theme::radius::CONTROL)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.0, color);
                ui.label(RichText::new(label).size(theme::type_scale::LABEL).strong());
            });
        });
}

fn model_option_matches(option: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || option.to_lowercase().contains(&query)
}

pub fn status_dot(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
        ui.painter().circle_filled(dot.center(), 3.5, color);
        ui.add_space(theme::space::XS);
        ui.label(label);
    });
}

#[cfg(test)]
mod tests {
    use super::model_option_matches;

    #[test]
    fn model_search_is_case_insensitive_and_trims_input() {
        assert!(model_option_matches("RealESRGAN x4", "  realesrgan "));
        assert!(model_option_matches("RIFE 4.25", ""));
        assert!(!model_option_matches("RIFE 4.25", "esrgan"));
    }
}
