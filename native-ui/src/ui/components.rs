use eframe::egui::{
    self, Align2, Color32, FontId, Id, Pos2, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, epaint::Mesh,
};

use super::{icons, theme};

pub fn palette_of(ui: &Ui) -> theme::Palette {
    theme::palette(!ui.visuals().dark_mode, ui.visuals().selection.stroke.color)
}

// ---------------------------------------------------------------- painting

/// Outline of a rounded rectangle, clockwise.
fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let radius = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [
        (rect.right() - radius, rect.top() + radius, -90.0_f32),
        (rect.right() - radius, rect.bottom() - radius, 0.0),
        (rect.left() + radius, rect.bottom() - radius, 90.0),
        (rect.left() + radius, rect.top() + radius, 180.0),
    ];
    let mut points = Vec::with_capacity(28);
    for (x, y, start) in corners {
        for step in 0..=6 {
            let angle = (start + step as f32 * 15.0).to_radians();
            points.push(Pos2::new(
                x + radius * angle.cos(),
                y + radius * angle.sin(),
            ));
        }
    }
    points
}

fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let blend = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount) as u8;
    Color32::from_rgba_premultiplied(
        blend(from.r(), to.r()),
        blend(from.g(), to.g()),
        blend(from.b(), to.b()),
        blend(from.a(), to.a()),
    )
}

/// A rounded rectangle with a left-to-right gradient.
pub fn gradient_rect(painter: &egui::Painter, rect: Rect, radius: f32, from: Color32, to: Color32) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    let shade = |point: Pos2| mix(from, to, (point.x - rect.left()) / rect.width());
    let outline = rounded_outline(rect, radius);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), shade(rect.center()));
    for point in &outline {
        mesh.colored_vertex(*point, shade(*point));
    }
    let count = outline.len() as u32;
    for index in 0..count {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
    }
    painter.add(Shape::mesh(mesh));
    // The mesh itself is not anti-aliased; a hairline outline smooths it.
    painter.add(Shape::closed_line(
        outline,
        Stroke::new(0.6, mix(from, to, 0.5)),
    ));
}

/// Soft radial light used for the ambient background.
pub fn glow(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    const SEGMENTS: u32 = 40;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, color);
    for index in 0..SEGMENTS {
        let angle = index as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(center + Vec2::angled(angle) * radius, Color32::TRANSPARENT);
    }
    for index in 0..SEGMENTS {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % SEGMENTS);
    }
    painter.add(Shape::mesh(mesh));
}

fn hover_amount(ui: &Ui, response: &Response) -> f32 {
    ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || response.has_focus(),
        ui.style().animation_time.min(theme::motion::HOVER_SECONDS),
    )
}

// ---------------------------------------------------------------- containers

/// A translucent panel.
pub fn glass_frame(palette: theme::Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(theme::radius())
        .inner_margin(egui::Margin::same(18))
}

pub fn card<R>(ui: &mut Ui, title: &str, content: impl FnOnce(&mut Ui) -> R) -> R {
    let palette = palette_of(ui);
    glass_frame(palette)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new(title)
                    .font(theme::title_font(theme::type_scale::SECTION))
                    .color(palette.text),
            );
            ui.add_space(theme::space::SM);
            content(ui)
        })
        .inner
}

pub fn icon_tile(ui: &mut Ui, icon: &str, color: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(
        rect,
        f32::from(theme::control_radius()),
        color.gamma_multiply(0.18),
    );
    ui.place(
        Rect::from_center_size(rect.center(), Vec2::splat(size * 0.55)),
        icons::styled(icon, color, size * 0.55),
    );
}

// ---------------------------------------------------------------- controls

pub fn primary_button_enabled(ui: &mut Ui, label: &str, enabled: bool, width: f32) -> Response {
    let palette = palette_of(ui);
    let width = width.min(ui.available_width().max(1.0));
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 42.0), sense);
    let hover = if enabled {
        hover_amount(ui, &response)
    } else {
        0.0
    };
    let pressed = response.is_pointer_button_down_on() && enabled;
    let body = rect.shrink(if pressed { 1.5 } else { 0.0 });
    let corner = f32::from(theme::control_radius()) + 2.0;
    if enabled {
        if hover > 0.0 {
            ui.painter().rect_filled(
                body.expand(4.0 * hover),
                corner + 4.0,
                palette.accent.gamma_multiply(0.22 * hover),
            );
        }
        gradient_rect(
            ui.painter(),
            body,
            corner,
            palette.accent,
            palette.accent_alt,
        );
    } else {
        ui.painter().rect_filled(body, corner, palette.elevated);
    }
    ui.painter().text(
        body.center(),
        Align2::CENTER_CENTER,
        label,
        theme::title_font(14.0),
        if enabled {
            theme::contrast_text(palette.accent)
        } else {
            palette.muted
        },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Pill that can be selected, with an optional leading icon.
pub fn chip(ui: &mut Ui, icon: Option<&str>, label: &str, selected: bool) -> Response {
    let palette = palette_of(ui);
    let font = FontId::proportional(13.0);
    let text_width = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(label.to_owned(), font.clone(), palette.text)
            .size()
            .x
    });
    let icon_width = if icon.is_some() { 21.0 } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(text_width + icon_width + 24.0, 32.0),
        Sense::click(),
    );
    let hover = hover_amount(ui, &response);
    if selected {
        gradient_rect(
            ui.painter(),
            rect,
            16.0,
            palette.accent.gamma_multiply(0.38),
            palette.accent_alt.gamma_multiply(0.24),
        );
    } else {
        ui.painter().rect_filled(rect, 16.0, palette.elevated);
    }
    ui.painter().rect_stroke(
        rect,
        16.0,
        Stroke::new(
            1.0,
            if selected {
                palette.accent
            } else {
                mix(palette.border, palette.accent, hover * 0.7)
            },
        ),
        StrokeKind::Inside,
    );
    let color = if selected {
        palette.text
    } else {
        palette.secondary
    };
    let mut x = rect.left() + 12.0;
    if let Some(icon) = icon {
        ui.place(
            Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(15.0)),
            icons::styled(icon, color, 15.0),
        );
        x += icon_width;
    }
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        color,
    );
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Segmented control with a sliding highlight. Returns true when changed.
pub fn segmented(ui: &mut Ui, id: &str, options: &[&str], selected: &mut usize) -> bool {
    let palette = palette_of(ui);
    let font = FontId::proportional(13.0);
    let widest = options
        .iter()
        .map(|label| {
            ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap((*label).to_owned(), font.clone(), palette.text)
                    .size()
                    .x
            })
        })
        .fold(0.0_f32, f32::max);
    let count = options.len().max(1) as f32;
    let segment = (widest + 26.0).min((ui.available_width() / count).max(1.0));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(segment * count, 34.0), Sense::hover());
    let corner = f32::from(theme::control_radius()) + 2.0;
    ui.painter().rect_filled(rect, corner, palette.elevated);
    let position = ui.ctx().animate_value_with_time(
        Id::unique(("segmented", id)),
        *selected as f32,
        ui.style().animation_time,
    );
    let pill = Rect::from_min_size(
        Pos2::new(rect.left() + position * segment, rect.top()),
        Vec2::new(segment, rect.height()),
    )
    .shrink(3.0);
    gradient_rect(
        ui.painter(),
        pill,
        corner - 2.0,
        palette.accent,
        palette.accent_alt,
    );
    let mut changed = false;
    for (index, label) in options.iter().enumerate() {
        let area = Rect::from_min_size(
            Pos2::new(rect.left() + index as f32 * segment, rect.top()),
            Vec2::new(segment, rect.height()),
        );
        let response = ui
            .interact(area, Id::unique(("segment", id, index)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() && *selected != index {
            *selected = index;
            changed = true;
        }
        ui.painter().text(
            area.center(),
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            if *selected == index {
                theme::contrast_text(palette.accent)
            } else {
                palette.secondary
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::RadioButton, true, *selected == index, *label)
        });
    }
    changed
}

pub fn toggle(
    ui: &mut Ui,
    value: &mut bool,
    label: &str,
    accent: Color32,
    reduce_motion: bool,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(42.0, 24.0), Sense::click());
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
            theme::motion::NAV_SECONDS
        },
    );
    let palette = theme::palette(!ui.visuals().dark_mode, accent);
    ui.painter().rect_filled(rect, 12.0, palette.elevated);
    if progress > 0.0 {
        gradient_rect(
            ui.painter(),
            rect,
            12.0,
            palette.accent.gamma_multiply(progress),
            palette.accent_alt.gamma_multiply(progress),
        );
    }
    ui.painter().rect_stroke(
        rect,
        12.0,
        Stroke::new(
            if response.has_focus() { 1.5 } else { 1.0 },
            if response.has_focus() {
                palette.accent
            } else {
                palette.border
            },
        ),
        StrokeKind::Outside,
    );
    let knob_x = egui::lerp((rect.left() + 12.0)..=(rect.right() - 12.0), progress);
    ui.painter()
        .circle_filled(Pos2::new(knob_x, rect.center().y), 8.5, Color32::WHITE);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *value, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Gradient progress bar. The value eases toward its target.
pub fn progress_bar(ui: &mut Ui, id: &str, fraction: f32, height: f32) {
    let palette = palette_of(ui);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(1.0), height),
        Sense::hover(),
    );
    let shown = ui.ctx().animate_value_with_time(
        Id::unique(("progress", id)),
        fraction.clamp(0.0, 1.0),
        ui.style().animation_time * 2.0,
    );
    ui.painter()
        .rect_filled(rect, height / 2.0, palette.elevated);
    if shown > 0.001 {
        let filled = Rect::from_min_size(
            rect.min,
            Vec2::new((rect.width() * shown).max(height), height),
        );
        gradient_rect(
            ui.painter(),
            filled,
            height / 2.0,
            palette.accent,
            palette.accent_alt,
        );
    }
}

// ---------------------------------------------------------------- layout kit

/// A card with an icon, a title, and a one-line explanation.
pub fn section<R>(
    ui: &mut Ui,
    icon: &str,
    title: &str,
    subtitle: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> R {
    let palette = palette_of(ui);
    glass_frame(palette)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                icon_tile(ui, icon, palette.accent, 40.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(
                        RichText::new(title)
                            .font(theme::title_font(theme::type_scale::SECTION))
                            .color(palette.text),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(subtitle).size(12.0).color(palette.secondary),
                        )
                        .wrap(),
                    );
                });
            });
            ui.add_space(theme::space::MD);
            content(ui)
        })
        .inner
}

/// One setting: icon and explanation on the left, its control on the right.
pub fn row(ui: &mut Ui, icon: &str, title: &str, detail: &str, control: impl FnOnce(&mut Ui)) {
    let palette = palette_of(ui);
    egui::Frame::new()
        .fill(palette.elevated.gamma_multiply(0.55))
        .corner_radius(theme::control_radius() + 2)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(34.0);
                ui.add(icons::styled(icon, palette.accent, 18.0));
                ui.add_space(2.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    control(ui);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add(
                                egui::Label::new(RichText::new(title).color(palette.text))
                                    .truncate(),
                            );
                            if !detail.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(detail).size(11.0).color(palette.muted),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(detail);
                            }
                        });
                    });
                });
            });
        });
}

/// A button with an icon in front of its label.
pub fn icon_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let palette = palette_of(ui);
    let color = if enabled { palette.text } else { palette.muted };
    ui.add_enabled(
        enabled,
        egui::Button::image_and_text(icons::styled(icon, color, 15.0).image(), label)
            .min_size(Vec2::new(0.0, 34.0)),
    )
}

/// A big value with a label and an icon.
pub fn stat(ui: &mut Ui, icon: &str, tone: Color32, label: &str, value: &str) {
    let palette = palette_of(ui);
    glass_frame(palette)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                icon_tile(ui, icon, tone, 38.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(label).size(11.5).color(palette.muted));
                    ui.add(
                        egui::Label::new(
                            RichText::new(value)
                                .font(theme::title_font(15.0))
                                .color(palette.text),
                        )
                        .truncate(),
                    )
                    .on_hover_text(value);
                });
            });
        });
}

/// A tinted message with an icon.
pub fn notice(ui: &mut Ui, icon: &str, tone: Color32, text: &str) {
    egui::Frame::new()
        .fill(tone.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, tone.gamma_multiply(0.40)))
        .corner_radius(theme::control_radius() + 2)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(icons::styled(icon, tone, 16.0));
                ui.add(egui::Label::new(RichText::new(text).size(12.0)).wrap());
            });
        });
}

/// Scrolling command output on a dark ground in either theme.
pub fn terminal<'a>(ui: &mut Ui, id: &str, lines: impl Iterator<Item = &'a str>, height: f32) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(6, 9, 14, 225))
        .corner_radius(theme::control_radius() + 2)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                for (red, green, blue) in [(255, 95, 86), (255, 189, 46), (39, 201, 63)] {
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        4.0,
                        Color32::from_rgb(red, green, blue),
                    );
                }
            });
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(height)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for line in lines {
                        let lower = line.to_ascii_lowercase();
                        let color = if lower.contains("error") || lower.contains("failed") {
                            Color32::from_rgb(255, 123, 133)
                        } else if lower.starts_with('$') || lower.contains("installing") {
                            Color32::from_rgb(137, 199, 255)
                        } else {
                            Color32::from_rgb(188, 198, 210)
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(line).monospace().size(10.5).color(color),
                            )
                            .wrap(),
                        );
                    }
                });
        });
}

/// A picture and two lines for a list that has nothing in it yet.
pub fn empty_state(ui: &mut Ui, art: &str, title: &str, detail: &str) {
    let palette = palette_of(ui);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(120.0, 56.0), Sense::hover());
        super::art::category(ui.painter(), rect, art, palette.accent.gamma_multiply(0.7));
        ui.label(RichText::new(title).font(theme::title_font(13.0)));
        ui.add(egui::Label::new(RichText::new(detail).size(11.5).color(palette.muted)).wrap());
    });
}

// ---------------------------------------------------------------- text bits

pub fn metadata_chip(ui: &mut Ui, text: &str, accent: Color32) {
    let palette = theme::palette(!ui.visuals().dark_mode, accent);
    egui::Frame::new()
        .fill(palette.elevated)
        .corner_radius(theme::control_radius())
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.5).color(palette.secondary));
        });
}

pub fn copyable_path(ui: &mut Ui, path: &str, placeholder: &str) {
    let palette = palette_of(ui);
    egui::Frame::new()
        .fill(palette.elevated)
        .corner_radius(theme::control_radius())
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            let available = ui.available_width().max(1.0);
            ui.set_min_width(available);
            let label = if path.is_empty() { placeholder } else { path };
            ui.horizontal(|ui| {
                let text_width = if path.is_empty() {
                    available
                } else {
                    (available - 66.0).max(1.0)
                };
                let response = ui.add_sized(
                    Vec2::new(text_width, 28.0),
                    egui::Label::new(
                        RichText::new(label)
                            .monospace()
                            .size(theme::type_scale::MONOSPACE)
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

pub fn searchable_select(
    ui: &mut Ui,
    id: Id,
    selected: &mut String,
    search: &mut String,
    options: &[String],
    installed: impl Fn(&str) -> bool,
) -> bool {
    let picker_width = ui.available_width().max(1.0);
    let label = if selected.is_empty() {
        "Choose a model"
    } else {
        selected.as_str()
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .width(picker_width)
        .truncate()
        .show_ui(ui, |ui| {
            ui.add_sized(
                Vec2::new(ui.available_width(), 34.0),
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
                                .color(palette_of(ui).secondary),
                        );
                    }
                    for option in visible {
                        let state = if installed(option) {
                            "Installed"
                        } else {
                            "Downloads on first use"
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
    let palette = palette_of(ui);
    let font = FontId::proportional(theme::type_scale::CAPTION);
    let text = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(label.to_owned(), font.clone(), palette.text)
            .size()
            .x
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(text + 34.0, 28.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 14.0, color.gamma_multiply(0.14));
    ui.painter().rect_stroke(
        rect,
        14.0,
        Stroke::new(1.0, color.gamma_multiply(0.35)),
        StrokeKind::Inside,
    );
    ui.painter()
        .circle_filled(rect.left_center() + Vec2::new(13.0, 0.0), 3.5, color);
    ui.painter().text(
        rect.left_center() + Vec2::new(23.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        font,
        palette.text,
    );
}

fn model_option_matches(option: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || option.to_lowercase().contains(&query)
}

pub fn status_dot(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 3.5, color);
        ui.add_space(theme::space::XS);
        ui.label(label);
    });
}

#[cfg(test)]
mod tests {
    use super::{model_option_matches, rounded_outline};
    use eframe::egui::{Pos2, Rect, Vec2};

    #[test]
    fn model_search_is_case_insensitive_and_trims_input() {
        assert!(model_option_matches("RealESRGAN x4", "  realesrgan "));
        assert!(model_option_matches("RIFE 4.25", ""));
        assert!(!model_option_matches("RIFE 4.25", "esrgan"));
    }

    #[test]
    fn rounded_outline_stays_inside_the_rect_even_with_a_huge_radius() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(40.0, 10.0));
        for point in rounded_outline(rect, 100.0) {
            assert!(rect.expand(0.01).contains(point), "{point:?}");
        }
    }
}
