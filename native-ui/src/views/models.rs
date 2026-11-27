//! Model downloader: browse by purpose, tick what you want, fetch it in one go.

use std::collections::BTreeSet;

use eframe::egui::{self, Align, Color32, Id, Layout, RichText, Sense, Stroke, Vec2};
use serde_json::{Value, json};

use crate::{
    NativeApp, SETTINGS_PAGE, can_start_model_download, format_bytes,
    ui::{art, components, icons, theme},
};

/// Key, label, colour of the icon tile.
const CATEGORIES: [(&str, &str, Color32); 6] = [
    ("upscale", "Upscale", Color32::from_rgb(88, 142, 248)),
    (
        "interpolate",
        "Interpolate",
        Color32::from_rgb(160, 112, 246),
    ),
    ("denoise", "Denoise", Color32::from_rgb(44, 190, 152)),
    ("decompress", "Decompress", Color32::from_rgb(240, 148, 60)),
    ("deblur", "Deblur", Color32::from_rgb(232, 90, 146)),
    (
        "scene_change",
        "Scene detect",
        Color32::from_rgb(222, 180, 62),
    ),
];
const BACKENDS: [&str; 3] = ["pytorch", "ncnn", "tensorrt"];
const SHOW: [&str; 3] = ["All", "Installed", "Not installed"];

/// A model is identified by its category and catalogue label.
type ModelKey = (String, String);

#[derive(Default)]
pub struct ModelBrowser {
    inventory: Value,
    loading: bool,
    error: Option<String>,
    backend: Option<usize>,
    category: Option<usize>,
    show: usize,
    search: String,
    selected: BTreeSet<ModelKey>,
    pending_delete: Option<ModelKey>,
}

impl ModelBrowser {
    /// Drop the listing; the selection survives so it can be resumed.
    pub fn release(&mut self) {
        self.inventory = Value::Null;
        self.loading = false;
    }

    /// Ask again the next time the page draws.
    pub fn invalidate(&mut self) {
        self.release();
        self.error = None;
    }

    pub fn received(&mut self, result: Result<Value, String>) {
        self.loading = false;
        match result {
            Ok(value) => {
                self.error = value["error"].as_str().map(str::to_owned);
                self.inventory = value;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn category_style(key: &str) -> (&'static str, Color32) {
    CATEGORIES
        .iter()
        .find(|(name, ..)| *name == key)
        .map_or(("Model", Color32::GRAY), |(_, label, color)| {
            (label, *color)
        })
}

/// Split "RIFE 4.6 (Fastest Model)" into a title and a note.
fn split_label(label: &str) -> (&str, &str) {
    match label.find('(') {
        Some(index) if index > 0 => (
            label[..index].trim(),
            label[index..].trim_matches(['(', ')', ' ']),
        ),
        _ => (label, ""),
    }
}

fn matches(model: &Value, category: Option<&str>, show: usize, query: &str) -> bool {
    let installed = model["installed"].as_bool() == Some(true);
    if category.is_some_and(|wanted| model["category"] != wanted) {
        return false;
    }
    if (show == 1 && !installed) || (show == 2 && installed) {
        return false;
    }
    query.is_empty()
        || ["label", "arch", "category"].iter().any(|field| {
            model[*field]
                .as_str()
                .is_some_and(|text| text.to_lowercase().contains(query))
        })
}

impl NativeApp {
    pub(crate) fn models_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let backend_index = *self.model_browser.backend.get_or_insert_with(|| {
            BACKENDS
                .iter()
                .position(|name| *name == self.backend)
                .unwrap_or(0)
        });
        let backend = BACKENDS[backend_index];
        if self.connected
            && !self.model_browser.loading
            && self.model_browser.error.is_none()
            && self.model_browser.inventory["backend"] != backend
        {
            self.model_browser.loading = true;
            self.get(&format!("/api/models/inventory?backend={backend}"));
        }
        let downloading = self.download["active"].as_bool() == Some(true);
        let items = self.model_browser.inventory["models"]
            .as_array()
            .filter(|_| self.model_browser.inventory["backend"] == backend)
            .cloned()
            .unwrap_or_default();

        self.model_toolbar(ui, &items, backend_index);
        if downloading || self.download["status"] == "error" {
            ui.add_space(theme::space::MD);
            self.download_panel(ui);
        }
        ui.add_space(theme::space::MD);

        let query = self.model_browser.search.trim().to_lowercase();
        let category = self.model_browser.category.map(|index| CATEGORIES[index].0);
        let visible = items
            .iter()
            .filter(|model| matches(model, category, self.model_browser.show, &query))
            .collect::<Vec<_>>();
        self.selection_bar(ui, &items, &visible, backend);

        if let Some(error) = self.model_browser.error.clone() {
            ui.colored_label(
                palette.error,
                format!("The model list could not be loaded: {error}"),
            );
            if ui.button("Try again").clicked() {
                self.model_browser.invalidate();
            }
        } else if !self.connected {
            ui.label("The model list appears when the local engine connects.");
        } else if self.model_browser.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking which models are installed…");
            });
        } else if visible.is_empty() {
            ui.label("No models match these filters.");
        }

        let columns = ((ui.available_width() / 320.0) as usize).clamp(1, 4);
        for row in visible.chunks(columns) {
            ui.columns(columns, |cells| {
                for (cell, model) in cells.iter_mut().zip(row) {
                    self.model_card(cell, model, downloading);
                }
            });
            ui.add_space(2.0);
        }
    }

    fn model_toolbar(&mut self, ui: &mut egui::Ui, items: &[Value], backend_index: usize) {
        let palette = self.palette();
        components::glass_frame(palette).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let mut selected = backend_index;
                if components::segmented(
                    ui,
                    "model-backend",
                    &["PyTorch", "NCNN", "TensorRT"],
                    &mut selected,
                ) {
                    self.model_browser.backend = Some(selected);
                    self.model_browser.selected.clear();
                    self.model_browser.invalidate();
                }
                ui.add_sized(
                    Vec2::new(ui.available_width().clamp(180.0, 320.0), 34.0),
                    egui::TextEdit::singleline(&mut self.model_browser.search)
                        .hint_text("Search models…")
                        .margin(Vec2::new(10.0, 8.0)),
                );
                components::segmented(ui, "model-show", &SHOW, &mut self.model_browser.show);
            });
            ui.add_space(theme::space::XS);
            ui.horizontal_wrapped(|ui| {
                if components::chip(
                    ui,
                    Some("all"),
                    &format!("All · {}", items.len()),
                    self.model_browser.category.is_none(),
                )
                .clicked()
                {
                    self.model_browser.category = None;
                }
                for (index, (key, label, _)) in CATEGORIES.iter().enumerate() {
                    let count = items.iter().filter(|m| m["category"] == *key).count();
                    if count == 0 {
                        continue;
                    }
                    if components::chip(
                        ui,
                        Some(key),
                        &format!("{label} · {count}"),
                        self.model_browser.category == Some(index),
                    )
                    .clicked()
                    {
                        self.model_browser.category = Some(index);
                    }
                }
            });
            ui.add_space(theme::space::XS);
            ui.horizontal_wrapped(|ui| {
                let installed = items
                    .iter()
                    .filter(|model| model["installed"].as_bool() == Some(true))
                    .count();
                let bytes: u64 = items.iter().filter_map(|m| m["bytes"].as_u64()).sum();
                ui.label(
                    RichText::new(format!(
                        "{installed} of {} installed · {} on disk · stored in {}",
                        items.len(),
                        if self.model_browser.inventory["sizes_known"] == false {
                            "size not measured".to_owned()
                        } else {
                            format_bytes(bytes)
                        },
                        if self.model_storage == "wsl" {
                            "WSL Ubuntu"
                        } else {
                            "Windows"
                        }
                    ))
                    .size(11.5)
                    .color(palette.muted),
                );
                if ui.small_button("Refresh").clicked() {
                    self.model_browser.invalidate();
                }
                if ui.small_button("Import your own…").clicked() {
                    self.query("/api/actions/import_custom_models", json!({}));
                }
                if ui.small_button("Open custom folder").clicked() {
                    self.query("/api/actions/open_custom_models_folder", json!({}));
                }
                if ui.small_button("Storage location").clicked() {
                    self.page = SETTINGS_PAGE;
                }
            });
        });
    }

    fn selection_bar(
        &mut self,
        ui: &mut egui::Ui,
        items: &[Value],
        visible: &[&Value],
        backend: &str,
    ) {
        let palette = self.palette();
        // Installed models cannot stay ticked.
        self.model_browser.selected.retain(|(category, label)| {
            !items.iter().any(|model| {
                model["category"] == category.as_str()
                    && model["label"] == label.as_str()
                    && model["installed"].as_bool() == Some(true)
            })
        });
        ui.horizontal_wrapped(|ui| {
            let count = self.model_browser.selected.len();
            if ui.button("Select all shown").clicked() {
                for model in visible {
                    if model["installed"].as_bool() != Some(true)
                        && let (Some(category), Some(label)) =
                            (model["category"].as_str(), model["label"].as_str())
                    {
                        self.model_browser
                            .selected
                            .insert((category.to_owned(), label.to_owned()));
                    }
                }
            }
            if ui
                .add_enabled(count > 0, egui::Button::new("Clear selection"))
                .clicked()
            {
                self.model_browser.selected.clear();
            }
            ui.label(RichText::new(format!("{count} selected")).color(palette.secondary));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let allowed = count > 0
                    && can_start_model_download(
                        self.connected,
                        self.download["active"].as_bool() == Some(true),
                        self.busy || self.benchmarks["status"] == "running",
                    );
                let label = if count > 1 {
                    format!("Download {count} models")
                } else {
                    "Download selected".to_owned()
                };
                if components::primary_button_enabled(ui, &label, allowed, 220.0).clicked() {
                    let items = self
                        .model_browser
                        .selected
                        .iter()
                        .map(|(category, label)| json!({"category": category, "label": label}))
                        .collect::<Vec<_>>();
                    self.post(
                        "/api/models/download",
                        json!({"backend": backend, "items": items}),
                    );
                }
            });
        });
        ui.add_space(theme::space::XS);
    }

    fn download_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::glass_frame(palette).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if let Some(job) = self
                .jobs()
                .into_iter()
                .find(|job| job.activity.id == "download")
            {
                self.job_block(ui, &job, false);
            }
            if let Some(error) = self.download["error"].as_str().filter(|e| !e.is_empty()) {
                components::notice(ui, "warning", palette.error, error);
            }
        });
    }

    fn model_card(&mut self, ui: &mut egui::Ui, model: &Value, downloading: bool) {
        let palette = self.palette();
        let category = model["category"].as_str().unwrap_or_default();
        let label = model["label"].as_str().unwrap_or("Unnamed model");
        let installed = model["installed"].as_bool() == Some(true);
        let custom = model["custom"].as_bool() == Some(true);
        let (category_label, color) = category_style(category);
        let (title, note) = split_label(label);
        let key = (category.to_owned(), label.to_owned());
        let ticked = self.model_browser.selected.contains(&key);
        let listed = |field: &str| {
            downloading
                && self.download[field]
                    .as_array()
                    .is_some_and(|list| list.iter().any(|item| item == label))
        };
        let current = downloading && self.download["label"] == label;
        let queued = listed("items") && !listed("done") && !current;
        let lit = ui.ctx().animate_bool_with_time(
            Id::unique(("model-tick", category, label)),
            ticked || current,
            ui.style().animation_time,
        );

        let frame = components::glass_frame(palette)
            .inner_margin(egui::Margin::same(13))
            .fill(if lit > 0.0 {
                color.gamma_multiply(0.14 * lit)
            } else {
                palette.surface
            })
            .stroke(Stroke::new(
                1.0,
                if lit > 0.0 {
                    color.gamma_multiply(lit)
                } else {
                    palette.border
                },
            ))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(150.0);
                art::banner(ui, category, color, 54.0);
                ui.horizontal(|ui| {
                    components::icon_tile(ui, category, color, 40.0);
                    ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                        let (mark, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                        if installed {
                            ui.painter().circle_filled(
                                mark.center(),
                                11.0,
                                palette.success.gamma_multiply(0.22),
                            );
                            ui.place(
                                mark.shrink(4.0),
                                icons::styled("check", palette.success, 14.0),
                            );
                        } else {
                            ui.painter().circle_stroke(
                                mark.center(),
                                9.0,
                                Stroke::new(1.5, if ticked { color } else { palette.muted }),
                            );
                            if ticked {
                                ui.painter().circle_filled(mark.center(), 5.0, color);
                            }
                        }
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.add(
                                egui::Label::new(
                                    RichText::new(title)
                                        .font(theme::title_font(13.5))
                                        .color(palette.text),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(if note.is_empty() {
                                        category_label
                                    } else {
                                        note
                                    })
                                    .size(11.0)
                                    .color(palette.secondary),
                                )
                                .truncate(),
                            );
                        });
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let accent = self.prefs.accent;
                    components::metadata_chip(ui, category_label, accent);
                    let scale = model["scale"].as_u64().unwrap_or(1);
                    if scale > 1 {
                        components::metadata_chip(ui, &format!("{scale}×"), accent);
                    }
                    if let Some(bytes) = model["bytes"].as_u64().filter(|bytes| *bytes > 0) {
                        components::metadata_chip(ui, &format_bytes(bytes), accent);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if current {
                            ui.spinner();
                        } else if queued {
                            ui.label(RichText::new("Queued").size(11.0).color(color));
                        } else if custom {
                            ui.label(RichText::new("Custom").size(11.0).color(palette.muted));
                        } else if installed
                            && ui
                                .add(egui::Button::new("Delete").small())
                                .on_hover_text("Remove this model from disk")
                                .clicked()
                        {
                            self.model_browser.pending_delete = Some(key.clone());
                        }
                    });
                });
            });

        if !installed {
            let response = ui
                .interact(
                    frame.response.rect,
                    Id::unique(("model-card", category, label)),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(label);
            if response.hovered() && !ticked {
                ui.painter().rect_stroke(
                    response.rect,
                    f32::from(theme::radius()),
                    Stroke::new(1.0, color.gamma_multiply(0.6)),
                    egui::StrokeKind::Inside,
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::CheckBox, true, ticked, label)
            });
            if response.clicked() && !self.model_browser.selected.remove(&key) {
                self.model_browser.selected.insert(key);
            }
        }
    }

    pub(crate) fn model_delete_dialog(&mut self, ctx: &egui::Context) {
        let Some((category, label)) = self.model_browser.pending_delete.clone() else {
            return;
        };
        let backend = BACKENDS[self.model_browser.backend.unwrap_or(0)];
        egui::Modal::new(Id::unique("model-delete")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.label(RichText::new("Delete this model?").font(theme::title_font(17.0)));
            ui.add_space(theme::space::SM);
            ui.label(format!(
                "{label} will be removed from disk. You can download it again later."
            ));
            ui.add_space(theme::space::MD);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    self.model_browser.pending_delete = None;
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Delete model"))
                    .clicked()
                {
                    self.model_browser.pending_delete = None;
                    self.post(
                        "/api/models/delete",
                        json!({"backend": backend, "category": category, "label": label}),
                    );
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{matches, split_label};
    use serde_json::json;

    #[test]
    fn labels_split_into_title_and_note() {
        assert_eq!(
            split_label("RIFE 4.6 (Fastest Model)"),
            ("RIFE 4.6", "Fastest Model")
        );
        assert_eq!(split_label("SPAN"), ("SPAN", ""));
    }

    #[test]
    fn filters_combine_category_state_and_search() {
        let model =
            json!({"label": "SPAN 2x", "category": "upscale", "arch": "span", "installed": true});
        assert!(matches(&model, Some("upscale"), 1, "span"));
        assert!(!matches(&model, Some("denoise"), 0, ""));
        assert!(!matches(&model, None, 2, ""));
        assert!(!matches(&model, None, 0, "rife"));
    }
}
