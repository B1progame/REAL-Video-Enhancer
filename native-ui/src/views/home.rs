//! Dashboard: start a job, see system health, reopen past work.

use eframe::egui::{self, Align, Layout, RichText, Vec2};
use serde_json::json;

use crate::{
    BENCHMARK_PAGE, NativeApp, PROCESS_PAGE, RUNTIME_PAGE, backend_is_installed,
    home_hero_uses_two_columns, home_status_uses_three_columns,
    prefs::file_name,
    ui::{components, theme},
};

impl NativeApp {
    pub(crate) fn home_page(&mut self, ui: &mut egui::Ui) {
        self.hero(ui);
        ui.add_space(theme::space::MD);
        self.stat_tiles(ui);
        ui.add_space(theme::space::MD);
        if ui.available_width() >= 760.0 {
            ui.columns(2, |columns| {
                self.recent_card(&mut columns[0]);
                self.history_card(&mut columns[1]);
            });
        } else {
            self.recent_card(ui);
            ui.add_space(theme::space::MD);
            self.history_card(ui);
        }
        ui.add_space(theme::space::MD);
        components::card(ui, "Current render", |ui| self.render_summary(ui));
    }

    fn hero(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::glass_frame(palette)
            .inner_margin(egui::Margin::same(26))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if home_hero_uses_two_columns(ui.available_width()) {
                    ui.columns(2, |columns| {
                        self.hero_text(&mut columns[0]);
                        self.source_stage(&mut columns[1], 230.0);
                    });
                } else {
                    self.hero_text(ui);
                    ui.add_space(theme::space::LG);
                    self.source_stage(ui, 200.0);
                }
            });
    }

    fn hero_text(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        ui.label(
            RichText::new("LOCAL AI VIDEO STUDIO")
                .font(theme::title_font(11.0))
                .color(palette.accent),
        );
        ui.add_space(theme::space::XS);
        ui.label(
            RichText::new("Make more of\nevery frame.")
                .font(theme::title_font(36.0))
                .color(palette.text),
        );
        ui.add_space(theme::space::XS);
        ui.label(
            RichText::new(
                "Upscale, smooth motion and restore detail. Your footage never leaves this PC.",
            )
            .size(14.0)
            .color(palette.secondary),
        );
        ui.add_space(theme::space::LG);
        ui.horizontal_wrapped(|ui| {
            let label = if self.input_path.is_empty() {
                "Choose a video"
            } else {
                "Continue editing"
            };
            if components::primary_button_enabled(ui, label, true, 190.0).clicked() {
                self.page = PROCESS_PAGE;
                if self.input_path.is_empty() {
                    self.get("/api/dialog/select_input");
                }
            }
            if ui
                .add(egui::Button::new("Queue a folder").min_size(Vec2::new(130.0, 42.0)))
                .clicked()
            {
                self.pick_queue_folder();
                self.page = PROCESS_PAGE;
            }
        });
    }

    fn stat_tiles(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let backends_ready = backend_is_installed(
            &self.bootstrap["system"]["available_backends"],
            &self.backend,
        );
        let gpu = self.bootstrap["system"]["gpu_name"]
            .as_str()
            .unwrap_or("Detecting…")
            .to_owned();
        let free = self.bootstrap["system"]["available_disk_gb"]
            .as_f64()
            .map_or_else(|| "—".to_owned(), |gb| format!("{gb:.0} GB"));
        let tiles = [
            (
                "process",
                "Backend",
                self.backend.to_uppercase(),
                if backends_ready {
                    "Ready to render"
                } else {
                    "Setup needed"
                },
                if backends_ready {
                    palette.success
                } else {
                    palette.warning
                },
                RUNTIME_PAGE,
            ),
            (
                "runtime",
                "Graphics",
                gpu,
                "Open benchmarks",
                palette.accent,
                BENCHMARK_PAGE,
            ),
            (
                "output",
                "Free space",
                free,
                if self.model_storage == "wsl" {
                    "Models live in WSL"
                } else {
                    "Models live in Windows"
                },
                palette.accent_alt,
                7,
            ),
        ];
        let mut open = None;
        let mut draw = |ui: &mut egui::Ui, index: usize| {
            let (icon, title, value, note, tone, page) = &tiles[index];
            let response = components::glass_frame(palette)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        components::icon_tile(ui, icon, *tone, 40.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.label(RichText::new(*title).size(11.5).color(palette.muted));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(value)
                                        .font(theme::title_font(16.0))
                                        .color(palette.text),
                                )
                                .truncate(),
                            );
                            ui.label(RichText::new(*note).size(11.5).color(*tone));
                        });
                    });
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.hovered() {
                ui.painter().rect_stroke(
                    response.rect,
                    f32::from(theme::radius()),
                    egui::Stroke::new(1.0, *tone),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                open = Some(*page);
            }
        };
        if home_status_uses_three_columns(ui.available_width()) {
            ui.columns(3, |columns| {
                for (index, column) in columns.iter_mut().enumerate() {
                    draw(column, index);
                }
            });
        } else {
            for index in 0..tiles.len() {
                draw(ui, index);
            }
        }
        if let Some(page) = open {
            self.page = page;
        }
        if !backends_ready
            && self.connected
            && components::primary_button_enabled(ui, "Install the runtime", !self.busy, 220.0)
                .clicked()
        {
            self.start_recommended_runtime_setup();
            self.page = RUNTIME_PAGE;
        }
    }

    fn recent_card(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let recent = self.prefs.recent.clone();
        components::card(ui, "Recent videos", |ui| {
            if recent.is_empty() {
                components::empty_state(
                    ui,
                    "scene_change",
                    "No videos yet",
                    "Videos you open appear here.",
                );
            }
            for path in recent.iter().take(5) {
                ui.horizontal(|ui| {
                    components::icon_tile(ui, "video", palette.accent, 28.0);
                    let open = ui
                        .add(egui::Button::new(file_name(path)).frame(false).truncate())
                        .on_hover_text(path);
                    if open.clicked() {
                        self.load_source(path);
                        self.page = PROCESS_PAGE;
                    }
                });
            }
        });
    }

    fn history_card(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let history = self.prefs.history.clone();
        components::card(ui, "Finished renders", |ui| {
            if history.is_empty() {
                components::empty_state(
                    ui,
                    "upscale",
                    "Nothing rendered yet",
                    "Finished files are listed here with a shortcut to their folder.",
                );
                return;
            }
            for entry in history.iter().take(5) {
                ui.horizontal(|ui| {
                    components::icon_tile(ui, "check", palette.success, 28.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Open folder").clicked() {
                            self.open_folder_of(&entry.output);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(egui::Label::new(file_name(&entry.output)).truncate())
                                .on_hover_text(format!("{}\nfrom {}", entry.output, entry.input));
                        });
                    });
                });
            }
            if ui.small_button("Clear history").clicked() {
                self.prefs.clear_history();
            }
        });
    }

    pub(crate) fn open_folder_of(&mut self, file: &str) {
        let folder = std::path::Path::new(file)
            .parent()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.query("/api/dialog/open_output", json!({"path": folder}));
    }
}
