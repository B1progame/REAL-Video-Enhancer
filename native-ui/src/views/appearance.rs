//! Appearance controls with a live miniature of the result.

use eframe::egui::{self, Color32, RichText, Sense, Vec2};

use crate::{
    NativeApp, settings_uses_two_columns,
    ui::{art, backdrop::Material, components, theme},
};

impl NativeApp {
    pub(crate) fn appearance_settings(&mut self, ui: &mut egui::Ui) {
        let before = self.prefs.clone();
        // Zoom is applied only when the slider is released, otherwise the
        // window rescales under the pointer while dragging.
        let mut zoom_settled = true;
        if settings_uses_two_columns(ui.available_width()) {
            ui.columns(2, |columns| {
                self.colour_section(&mut columns[0]);
                columns[0].add_space(theme::space::MD);
                self.window_section(&mut columns[0]);
                self.preview_section(&mut columns[1]);
                columns[1].add_space(theme::space::MD);
                zoom_settled = self.shape_section(&mut columns[1]);
            });
        } else {
            self.preview_section(ui);
            ui.add_space(theme::space::MD);
            self.colour_section(ui);
            ui.add_space(theme::space::MD);
            self.window_section(ui);
            ui.add_space(theme::space::MD);
            zoom_settled = self.shape_section(ui);
        }

        let zoom_pending = self.prefs.zoom != ui.ctx().zoom_factor()
            && !ui.input(|input| input.pointer.any_down());
        if zoom_settled && (self.prefs != before || zoom_pending) {
            self.apply_look(ui.ctx());
            self.prefs.save_look();
        }
    }

    fn preview_section(&mut self, ui: &mut egui::Ui) {
        components::section(
            ui,
            "eye",
            "Preview",
            "How the current look comes together.",
            |ui| art::theme_preview(ui, 190.0),
        );
    }

    fn colour_section(&mut self, ui: &mut egui::Ui) {
        components::section(
            ui,
            "palette",
            "Colour",
            "Theme and accent gradient.",
            |ui| {
                let mut theme_index = usize::from(self.prefs.appearance == "light");
                if components::segmented(ui, "theme", &["Dark", "Light"], &mut theme_index) {
                    self.prefs.appearance = ["dark", "light"][theme_index].to_owned();
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (name, primary, second) in theme::ACCENTS {
                        let color = Color32::from_rgb(primary[0], primary[1], primary[2]);
                        let partner = Color32::from_rgb(second[0], second[1], second[2]);
                        let selected = self.prefs.accent == color;
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(38.0, 34.0), Sense::click());
                        let grow = ui.ctx().animate_bool_with_time(
                            response.id,
                            selected || response.hovered(),
                            ui.style().animation_time,
                        );
                        components::gradient_rect(
                            ui.painter(),
                            rect.shrink(4.0 - 3.0 * grow),
                            12.0,
                            color,
                            partner,
                        );
                        if selected {
                            ui.painter()
                                .circle_filled(rect.center(), 5.0, Color32::WHITE);
                        }
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::RadioButton,
                                true,
                                selected,
                                name,
                            )
                        });
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(name)
                            .clicked()
                        {
                            self.prefs.accent = color;
                        }
                    }
                });
                let accent = self.prefs.accent;
                components::row(ui, "palette", "Custom colour", "Pick any accent.", |ui| {
                    let mut custom = [accent.r(), accent.g(), accent.b()];
                    if ui.color_edit_button_srgb(&mut custom).changed() {
                        self.prefs.accent = Color32::from_rgb(custom[0], custom[1], custom[2]);
                    }
                });
            },
        );
    }

    fn window_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::section(
            ui,
            "layers",
            "Window material",
            "What shows through from the desktop.",
            |ui| {
                let mut material = Material::ALL
                    .iter()
                    .position(|m| *m == self.prefs.material)
                    .unwrap_or(0);
                let labels = Material::ALL.map(Material::label);
                if components::segmented(ui, "material", &labels, &mut material) {
                    self.prefs.material = Material::ALL[material];
                }
                if self.prefs.material != Material::Solid {
                    if !self.material_active {
                        components::notice(
                            ui,
                            "warning",
                            palette.warning,
                            "This Windows version does not offer that material, so the window stays solid.",
                        );
                    }
                    let glass = &mut self.prefs.glass;
                    components::row(ui, "eye", "See-through", "How much shows through.", |ui| {
                        ui.add(egui::Slider::new(glass, 0.0..=1.0).show_value(false));
                    });
                    ui.label(
                        RichText::new("Windows turns this off in battery saver, or when Transparency effects is disabled.")
                            .size(11.0)
                            .color(palette.muted),
                    );
                }
            },
        );
    }

    /// Returns whether the interface-size slider has been released.
    fn shape_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut zoom_settled = true;
        components::section(
            ui,
            "sliders",
            "Shape and motion",
            "Corners, size and effects.",
            |ui| {
                let radius = &mut self.prefs.radius;
                components::row(ui, "all", "Corner roundness", "", |ui| {
                    ui.add(egui::Slider::new(radius, 0..=24));
                });
                let zoom = &mut self.prefs.zoom;
                components::row(ui, "upscale", "Interface size", "", |ui| {
                    let slider = ui.add(
                        egui::Slider::new(zoom, 0.8..=1.4)
                            .step_by(0.05)
                            .custom_formatter(|value, _| format!("{:.0}%", value * 100.0)),
                    );
                    zoom_settled = !slider.dragged();
                });
                let accent = self.prefs.accent;
                let glow = &mut self.prefs.glow;
                components::row(ui, "sparkles", "Ambient light", "Soft accent glow.", |ui| {
                    components::toggle(ui, glow, "Ambient light", accent, false);
                });
                let still = &mut self.prefs.reduce_motion;
                components::row(ui, "slomo", "Reduce motion", "Turn off animation.", |ui| {
                    components::toggle(ui, still, "Reduce motion", accent, false);
                });
            },
        );
        zoom_settled
    }
}
