//! The editing workspace: source stage, enhancements, queue, and render dock.

use std::path::Path;

use eframe::egui::{self, Align, Align2, Color32, Layout, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::{Value, json};

use crate::{
    NativeApp, SETTINGS_PAGE,
    prefs::{QueueAction, QueueInput, file_name},
    process_uses_two_columns,
    ui::{components, icons, progress, theme},
};

const ENHANCEMENTS: [(&str, &str, &str); 5] = [
    ("upscale", "Upscale", "Increase resolution"),
    ("interpolate", "Interpolate", "Smoother motion"),
    ("decompress", "Decompress", "Undo compression damage"),
    ("denoise", "Denoise", "Reduce grain and noise"),
    ("deblur", "Deblur", "Recover edges"),
];

fn duration_label(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    if total >= 3600 {
        format!("{}:{:02}:{:02}", total / 3600, total / 60 % 60, total % 60)
    } else {
        format!("{:02}:{:02}", total / 60, total % 60)
    }
}

/// Frames the renderer is expected to write, if the source is known.
pub fn expected_frames(info: &Value, interpolate: Option<f32>) -> Option<f64> {
    let frames = info["fps"].as_f64()? * info["duration_seconds"].as_f64()?;
    let frames = frames * f64::from(interpolate.unwrap_or(1.0));
    (frames >= 1.0).then_some(frames)
}

impl NativeApp {
    /// Load a file into the editor and inspect it.
    pub(crate) fn load_source(&mut self, path: &str) {
        let output = Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned());
        self.input_path = path.to_owned();
        if !self.queue.running()
            && let Some(output) = output
        {
            self.output_folder = output;
        }
        self.inspect_source();
    }

    /// Never blocked by other work, so a dropped file cannot be lost.
    pub(crate) fn inspect_source(&mut self) {
        self.source_preview = None;
        self.source_preview_path.clear();
        self.source_preview_error = None;
        self.input_info = json!({"status": "inspecting"});
        let path = self.input_path.trim().to_owned();
        self.query("/api/video/inspect", json!({"path": path}));
    }

    pub(crate) fn pick_queue_folder(&mut self) {
        self.query("/api/dialog/select_inputs", json!({}));
    }

    /// The preview frame, or a drop target while nothing is loaded.
    pub(crate) fn source_stage(&mut self, ui: &mut egui::Ui, max_height: f32) {
        let palette = self.palette();
        let width = ui.available_width().max(1.0);
        let height = (width * 9.0 / 16.0).clamp(180.0, max_height.max(180.0));
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
        let corner = f32::from(theme::radius());
        let empty = self.input_path.trim().is_empty();
        let hover = ui.ctx().animate_bool_with_time(
            response.id,
            response.hovered(),
            ui.style().animation_time,
        );
        let preview = self
            .source_preview
            .as_ref()
            .filter(|_| self.source_preview_path == self.input_path.trim());
        if let Some(texture) = preview {
            ui.painter().rect_filled(rect, corner, Color32::BLACK);
            ui.place(
                rect,
                egui::Image::new(texture)
                    .fit_to_exact_size(rect.size())
                    .maintain_aspect_ratio(true)
                    .corner_radius(theme::radius()),
            );
        } else {
            ui.painter().rect_filled(rect, corner, palette.elevated);
            ui.painter().rect_stroke(
                rect.shrink(8.0),
                corner,
                Stroke::new(
                    1.0 + hover,
                    palette.accent.gamma_multiply(0.35 + 0.5 * hover),
                ),
                egui::StrokeKind::Inside,
            );
            let (title, detail) = if empty {
                (
                    "Drop a video here",
                    "or click to browse · several files go to the queue",
                )
            } else if self.source_preview_loading || self.input_info["status"] == "inspecting" {
                ("Preparing preview…", "Reading a frame from your video")
            } else {
                ("No preview", "This video could not be previewed")
            };
            let center = rect.center();
            ui.painter().circle_filled(
                center - Vec2::new(0.0, 38.0),
                28.0 + hover * 3.0,
                palette.accent.gamma_multiply(0.20),
            );
            ui.place(
                Rect::from_center_size(center - Vec2::new(0.0, 38.0), Vec2::splat(28.0)),
                icons::styled("video", palette.accent, 28.0),
            );
            ui.painter().text(
                center + Vec2::new(0.0, 12.0),
                Align2::CENTER_CENTER,
                title,
                theme::title_font(16.0),
                palette.text,
            );
            ui.painter().text(
                center + Vec2::new(0.0, 36.0),
                Align2::CENTER_CENTER,
                detail,
                egui::FontId::proportional(12.0),
                palette.secondary,
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Choose source video")
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to choose a video")
            .clicked()
        {
            self.get("/api/dialog/select_input");
        }
    }

    fn source_card(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::card(ui, "Source", |ui| {
            self.source_stage(ui, 380.0);
            if !self.input_path.trim().is_empty() {
                ui.add_space(theme::space::XS);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Add to queue").clicked() {
                            let path = self.input_path.trim().to_owned();
                            if self.queue.add(&path) {
                                self.show_toast(
                                    "Added to queue",
                                    file_name(&path).to_owned(),
                                    crate::ToastKind::Success,
                                );
                            }
                        }
                        if ui.button("Replace").clicked() {
                            self.get("/api/dialog/select_input");
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(file_name(self.input_path.trim()))
                                        .font(theme::title_font(14.0))
                                        .color(palette.text),
                                )
                                .truncate(),
                            )
                            .on_hover_text(self.input_path.trim());
                        });
                    });
                });
            }
            let info = &self.input_info;
            if let Some(error) = info["error"].as_str() {
                ui.colored_label(
                    palette.warning,
                    format!("Video details could not be read: {error}"),
                );
            } else if let (Some(width), Some(height)) =
                (info["width"].as_u64(), info["height"].as_u64())
            {
                let accent = self.prefs.accent;
                ui.horizontal_wrapped(|ui| {
                    components::metadata_chip(ui, &format!("{width} × {height}"), accent);
                    if let Some(fps) = info["fps"].as_f64() {
                        components::metadata_chip(ui, &format!("{fps:.2} fps"), accent);
                    }
                    if let Some(seconds) = info["duration_seconds"].as_f64() {
                        components::metadata_chip(ui, &duration_label(seconds), accent);
                    }
                    if let Some(codec) = info["codec"].as_str() {
                        components::metadata_chip(ui, &codec.to_uppercase(), accent);
                    }
                    if let Some(bytes) = info["size_bytes"].as_u64() {
                        components::metadata_chip(ui, &crate::format_bytes(bytes), accent);
                    }
                    let scale = if self.toggles.get("upscale") == Some(&true) {
                        self.selected_scale()
                    } else {
                        1
                    };
                    if scale > 1 {
                        components::metadata_chip(
                            ui,
                            &format!(
                                "→ {} × {}",
                                width * u64::from(scale),
                                height * u64::from(scale)
                            ),
                            accent,
                        );
                    }
                });
            }
            if let Some(error) = self.source_preview_error.as_deref() {
                ui.colored_label(palette.warning, format!("Preview: {error}"));
            }
            egui::CollapsingHeader::new("Enter a video path manually")
                .id_salt("manual-video-path")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let field = (ui.available_width() - 84.0).max(1.0);
                        let response = ui.add_sized(
                            Vec2::new(field, 34.0),
                            egui::TextEdit::singleline(&mut self.input_path)
                                .hint_text("Paste a video file path"),
                        );
                        if response.changed() {
                            self.input_info = Value::Null;
                            self.source_preview = None;
                            self.source_preview_path.clear();
                        }
                        let submitted = response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        if ui.button("Inspect").clicked() || submitted {
                            self.inspect_source();
                        }
                    });
                });
        });
    }

    /// Output scale of the selected upscaler, from the override or its name.
    fn selected_scale(&self) -> u32 {
        if self.upscale_scale > 0 {
            return self.upscale_scale as u32;
        }
        let name = self
            .selected_models
            .get("upscale")
            .map(|name| name.to_ascii_lowercase())
            .unwrap_or_default();
        [4_u32, 3, 2]
            .into_iter()
            .find(|scale| {
                name.contains(&format!("{scale}x")) || name.contains(&format!("x{scale}"))
            })
            .unwrap_or(1)
    }

    fn enhancement_card(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let mut changed = false;
        components::card(ui, "Enhancements", |ui| {
            if !self.connected {
                ui.label(
                    RichText::new("Model choices appear when the local engine connects.")
                        .size(11.5)
                        .color(palette.warning),
                );
            }
            for (key, label, detail) in ENHANCEMENTS {
                let enabled = self.toggles.get(key).copied().unwrap_or(false);
                let lit = ui.ctx().animate_bool_with_time(
                    egui::Id::unique(("enhancement", key)),
                    enabled,
                    ui.style().animation_time,
                );
                egui::Frame::new()
                    .fill(if enabled {
                        palette.accent.gamma_multiply(0.10 * lit)
                    } else {
                        palette.elevated.gamma_multiply(0.5)
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if enabled {
                            palette.accent.gamma_multiply(0.55 * lit)
                        } else {
                            palette.border
                        },
                    ))
                    .corner_radius(theme::control_radius() + 3)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            components::icon_tile(
                                ui,
                                key,
                                if enabled {
                                    palette.accent
                                } else {
                                    palette.muted
                                },
                                34.0,
                            );
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(
                                    RichText::new(label)
                                        .font(theme::title_font(13.5))
                                        .color(palette.text),
                                );
                                ui.label(RichText::new(detail).size(11.5).color(palette.secondary));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let value = self.toggles.entry(key.to_owned()).or_insert(false);
                                changed |= components::toggle(
                                    ui,
                                    value,
                                    label,
                                    self.prefs.accent,
                                    self.prefs.reduce_motion,
                                )
                                .changed();
                            });
                        });
                        // Options only take space while the step is switched on.
                        if enabled
                            && self.render_engine == "rve"
                            && !self.model_choices(key).is_empty()
                        {
                            changed |= self.model_picker(ui, key);
                            let selected =
                                self.selected_models.get(key).cloned().unwrap_or_default();
                            if !selected.is_empty()
                                && self.models["model_status"][key][&selected].as_bool()
                                    != Some(true)
                            {
                                components::status_dot(
                                    ui,
                                    palette.warning,
                                    "Downloads automatically when you render",
                                );
                            }
                        }
                    });
            }
            ui.horizontal(|ui| {
                ui.add(icons::styled("slomo", palette.secondary, 17.0));
                ui.label("Slow motion output");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let value = self.toggles.entry("slomo".to_owned()).or_insert(false);
                    changed |= components::toggle(
                        ui,
                        value,
                        "Slow motion output",
                        self.prefs.accent,
                        self.prefs.reduce_motion,
                    )
                    .changed();
                });
            });
            if self.render_engine == "animescripter" {
                ui.label(
                    RichText::new(
                        "AnimeScripter downloads its own models the first time a method runs.",
                    )
                    .size(11.0)
                    .color(palette.muted),
                );
            } else if ui.button("Browse and download models").clicked() {
                self.page = 2;
            }
        });
        if changed {
            self.save_render_defaults();
        }
    }

    fn queue_card(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::card(ui, "Render queue", |ui| {
            ui.label(
                RichText::new("Every file uses the enhancements and output folder set here.")
                    .size(11.5)
                    .color(palette.secondary),
            );
            let running = self.queue.running();
            let mut remove = None;
            for (index, path) in self.queue.items.iter().enumerate() {
                ui.horizontal(|ui| {
                    let current = running && index == 0;
                    components::icon_tile(
                        ui,
                        if current { "play" } else { "video" },
                        if current {
                            palette.accent
                        } else {
                            palette.muted
                        },
                        24.0,
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(!current, egui::Button::new("Remove").small())
                            .clicked()
                        {
                            remove = Some(index);
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.add(egui::Label::new(file_name(path)).truncate())
                                .on_hover_text(path);
                        });
                    });
                });
            }
            if let Some(index) = remove {
                self.queue.items.remove(index);
            }
            if let Some(note) = &self.queue.note {
                ui.label(RichText::new(note).size(12.0).color(palette.secondary));
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Add folder").clicked() {
                    self.pick_queue_folder();
                }
                if running {
                    if ui.button("Stop after current").clicked() {
                        self.queue.stop("Queue stopped.");
                    }
                } else {
                    if ui
                        .add_enabled(
                            !self.queue.items.is_empty() && self.connected,
                            egui::Button::new("Run queue"),
                        )
                        .clicked()
                    {
                        // The queue starts a fresh render, so forget the last result.
                        if matches!(
                            self.render["status"].as_str(),
                            Some("completed" | "error" | "stopped")
                        ) {
                            self.render["status"] = json!("idle");
                        }
                        self.queue.start();
                    }
                    if ui
                        .add_enabled(!self.queue.items.is_empty(), egui::Button::new("Clear"))
                        .clicked()
                    {
                        self.queue.items.clear();
                        self.queue.note = None;
                    }
                }
            });
        });
    }

    /// Advance the queue. Called once per frame.
    pub(crate) fn drive_queue(&mut self) {
        if !self.queue.running() {
            return;
        }
        let status = self.render["status"].as_str().unwrap_or("idle").to_owned();
        let current = self.queue.items.front().cloned().unwrap_or_default();
        let loaded = self.input_path == current && self.input_info["width"].is_u64();
        let failure = if self.input_path == current && self.input_info["error"].is_string() {
            Some(format!("{} could not be read.", file_name(&current)))
        } else if loaded && !self.busy {
            self.render_blockers()
                .into_iter()
                .find(|blocker| blocker != "A render is already active.")
        } else {
            None
        };
        let action = self.queue.step(&QueueInput {
            render_status: &status,
            ready: loaded && !self.busy && self.render_blockers().is_empty(),
            failure,
        });
        match action {
            QueueAction::None => {}
            QueueAction::Load(path) => self.load_source(&path),
            QueueAction::Start => {
                // Only an error from this attempt should pause the queue.
                self.render["status"] = json!("idle");
                self.render["error"] = Value::Null;
                self.start_render();
            }
        }
    }

    pub(crate) fn studio_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        if self.first_setup {
            components::glass_frame(palette).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("First setup").font(theme::title_font(14.0)));
                    ui.label("Choose model storage and prepare the runtime before rendering.");
                    if ui.button("Set up now").clicked() {
                        self.page = SETTINGS_PAGE;
                    }
                });
            });
            ui.add_space(theme::space::MD);
        }
        let two_columns = process_uses_two_columns(ui.available_width());
        if two_columns {
            ui.columns(2, |columns| {
                self.studio_left(&mut columns[0]);
                self.studio_right(&mut columns[1]);
            });
        } else {
            self.studio_left(ui);
            ui.add_space(theme::space::MD);
            self.studio_right(ui);
        }
    }

    fn studio_left(&mut self, ui: &mut egui::Ui) {
        self.source_card(ui);
        ui.add_space(theme::space::MD);
        self.live_render_preview_card(ui);
        self.output_configuration_card(ui);
        ui.add_space(theme::space::MD);
        self.queue_card(ui);
        if let Some(lines) = self.render["log_lines"].as_array() {
            ui.add_space(theme::space::SM);
            egui::CollapsingHeader::new("Render activity")
                .id_salt("render-activity")
                .show(ui, |ui| {
                    for line in lines.iter().rev().take(8).filter_map(Value::as_str) {
                        ui.label(RichText::new(line).monospace().size(11.0));
                    }
                });
        }
    }

    fn studio_right(&mut self, ui: &mut egui::Ui) {
        self.processing_engine_card(ui);
        ui.add_space(theme::space::MD);
        self.enhancement_card(ui);
        ui.add_space(theme::space::MD);
        self.advanced_processing_card(ui);
    }

    pub(crate) fn render_dock(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let status = self.render["status"].as_str().unwrap_or("idle").to_owned();
        let active = matches!(
            status.as_str(),
            "preparing" | "running" | "paused" | "stopping"
        );
        let paused = status == "paused";
        let blockers = self.render_blockers();
        let (title, tone) = match status.as_str() {
            "preparing" => ("Preparing render", palette.accent),
            "paused" => ("Paused", palette.warning),
            "stopping" => ("Stopping…", palette.warning),
            "running" => ("Rendering", palette.accent),
            _ if status == "error" => ("Render needs attention", palette.error),
            _ if !blockers.is_empty() => ("Not ready yet", palette.warning),
            "completed" => ("Render complete", palette.success),
            _ => ("Ready to render", palette.success),
        };
        let job = self
            .jobs()
            .into_iter()
            .find(|job| job.activity.id == "render");
        let timing = job.as_ref().map(|job| self.job_timing(job));
        let fraction = job.as_ref().and_then(|job| job.activity.fraction);
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.set_min_height(44.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if active {
                    let pausable = status != "preparing"
                        && status != "stopping"
                        && self.render["engine"].as_str() != Some("animescripter");
                    if ui
                        .add_enabled(
                            status != "stopping",
                            egui::Button::new("Cancel").min_size(Vec2::new(90.0, 42.0)),
                        )
                        .clicked()
                    {
                        self.queue.stop("Queue stopped.");
                        self.post("/api/render/stop", json!({}));
                    }
                    if pausable {
                        let label = if paused { "Resume" } else { "Pause" };
                        if components::primary_button_enabled(ui, label, true, 140.0).clicked() {
                            self.post("/api/render/pause", json!({"paused": !paused}));
                        }
                    }
                } else {
                    if components::primary_button_enabled(
                        ui,
                        "Start render",
                        blockers.is_empty() && !self.queue.running(),
                        180.0,
                    )
                    .clicked()
                    {
                        self.start_render();
                    }
                    if status == "completed"
                        && let Some(output) = self.render["output_file"].as_str()
                    {
                        let output = output.to_owned();
                        if ui
                            .add(egui::Button::new("Open folder").min_size(Vec2::new(100.0, 42.0)))
                            .clicked()
                        {
                            self.open_folder_of(&output);
                        }
                    }
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        components::status_dot(ui, tone, title);
                        let detail = if let (Some(job), Some(timing)) = (&job, timing) {
                            let mut parts = Vec::new();
                            if let Some(fraction) = fraction {
                                parts.push(format!("{:.0}%", fraction * 100.0));
                            }
                            parts.push(progress::timing_text(
                                timing,
                                fraction,
                                job.eta.as_deref(),
                                job.forecast,
                            ));
                            parts.extend(job.activity.facts.iter().cloned());
                            parts.join("  ·  ")
                        } else if let Some(error) =
                            self.render["error"].as_str().filter(|e| !e.is_empty())
                        {
                            error.to_owned()
                        } else {
                            blockers.first().cloned().unwrap_or_else(|| {
                                "Source, enhancements and output are set.".to_owned()
                            })
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(&detail).size(11.5).color(palette.secondary),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&detail);
                    });
                    if !active && let Some(blocker) = blockers.first() {
                        self.render_blocker_action(ui, blocker);
                    }
                });
            });
        });
        if active {
            progress::bar(ui, "render", fraction, 6.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{duration_label, expected_frames};
    use serde_json::json;

    #[test]
    fn expected_frames_includes_the_interpolation_factor() {
        let info = json!({"fps": 24.0, "duration_seconds": 10.0});
        assert_eq!(expected_frames(&info, None), Some(240.0));
        assert_eq!(expected_frames(&info, Some(2.0)), Some(480.0));
        assert_eq!(expected_frames(&json!({}), None), None);
    }

    #[test]
    fn durations_switch_to_hours() {
        assert_eq!(duration_label(75.0), "01:15");
        assert_eq!(duration_label(3725.0), "1:02:05");
    }
}
