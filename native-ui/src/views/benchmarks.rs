//! Local model benchmarks: setup, the fastest results, and the full table.

use eframe::egui::{self, RichText, Sense, Vec2};
use serde_json::{Value, json};

use crate::{
    NativeApp, ToastKind, backend_is_installed,
    ui::{components, theme},
};

const CATEGORIES: [(&str, &str); 5] = [
    ("upscale", "Upscale"),
    ("interpolate", "Interpolate"),
    ("decompress", "Decompress"),
    ("denoise", "Denoise"),
    ("deblur", "Deblur"),
];
const RESOLUTIONS: [&str; 5] = ["480p", "720p", "1080p", "1440p", "4K"];
const BACKENDS: [&str; 3] = ["pytorch", "ncnn", "tensorrt"];
const CLIPS: [usize; 3] = [24, 48, 96];

/// The quickest completed measurements for a backend: (model, resolution, fps).
fn fastest<'a>(results: &'a Value, backend: &str, count: usize) -> Vec<(&'a str, &'a str, f64)> {
    let mut best = results
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["backend"] == backend && item["status"] == "complete")
        .filter_map(|item| {
            Some((
                item["model"].as_str()?,
                item["resolution"].as_str()?,
                item["measured_fps"].as_f64()?,
            ))
        })
        .collect::<Vec<_>>();
    best.sort_by(|a, b| b.2.total_cmp(&a.2));
    best.truncate(count);
    best
}

#[derive(Debug, PartialEq)]
enum Cell<'a> {
    Unmeasured,
    Done(f64),
    Failed(&'a str),
}

/// The newest result for one model at one resolution.
fn cell<'a>(
    results: &'a Value,
    backend: &str,
    category: &str,
    model: &str,
    resolution: &str,
) -> Cell<'a> {
    let latest = results.as_array().and_then(|items| {
        items.iter().rev().find(|item| {
            item["backend"] == backend
                && item["category"] == category
                && item["model"] == model
                && item["resolution"] == resolution
        })
    });
    match latest {
        Some(item) if item["status"] == "complete" => {
            Cell::Done(item["measured_fps"].as_f64().unwrap_or(0.0))
        }
        Some(item) if item["status"] == "error" => {
            Cell::Failed(item["error"].as_str().unwrap_or("Benchmark failed."))
        }
        _ => Cell::Unmeasured,
    }
}

/// A card that opens to show the models of one purpose.
fn category_group(
    ui: &mut egui::Ui,
    category: &str,
    title: &str,
    ready: usize,
    total: usize,
    forced_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let palette = components::palette_of(ui);
    let id = ui.make_persistent_id(("benchmark-group", category));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let open = state.is_open() || forced_open;
    egui::Frame::new()
        .fill(palette.elevated.gamma_multiply(0.55))
        .stroke(egui::Stroke::new(1.0, palette.border))
        .corner_radius(theme::control_radius() + 3)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let header = ui
                .horizontal(|ui| {
                    components::icon_tile(ui, category, palette.accent, 34.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(
                            RichText::new(title)
                                .font(theme::title_font(14.0))
                                .color(palette.text),
                        );
                        ui.label(
                            RichText::new(format!("{ready} of {total} installed"))
                                .size(11.5)
                                .color(if ready > 0 {
                                    palette.success
                                } else {
                                    palette.muted
                                }),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (arrow, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                        let center = arrow.center();
                        let tip = if open { -3.0 } else { 3.0 };
                        ui.painter().add(egui::Shape::line(
                            vec![
                                center + Vec2::new(-5.0, -tip),
                                center + Vec2::new(0.0, tip),
                                center + Vec2::new(5.0, -tip),
                            ],
                            egui::Stroke::new(1.8, palette.secondary),
                        ));
                    });
                })
                .response;
            let click = ui
                .interact(header.rect, id.with("header"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            click.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, title));
            if click.clicked() && !forced_open {
                state.toggle(ui);
            }
            if forced_open {
                ui.add_space(theme::space::XS);
                body(ui);
                state.store(ui.ctx());
            } else {
                state.show_body_unindented(ui, |ui| {
                    ui.add_space(theme::space::XS);
                    body(ui);
                });
            }
        });
}

/// One model: its name and a pill per input resolution.
fn model_row(ui: &mut egui::Ui, name: &str, installed: bool, cells: &[(&str, Cell)]) {
    let palette = components::palette_of(ui);
    egui::Frame::new()
        .fill(palette.surface)
        .corner_radius(theme::control_radius() + 1)
        .inner_margin(egui::Margin::symmetric(11, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let dot = if installed {
                    palette.success
                } else {
                    palette.muted
                };
                let (mark, response) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                ui.painter().circle_filled(mark.center(), 4.0, dot);
                response.on_hover_text(if installed {
                    "Installed"
                } else {
                    "Not installed"
                });
                ui.add(
                    egui::Label::new(RichText::new(name).size(12.5).color(if installed {
                        palette.text
                    } else {
                        palette.secondary
                    }))
                    .truncate(),
                )
                .on_hover_text(name);
            });
            if !installed {
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (resolution, state) in cells {
                    let (value, tone, hint) = match state {
                        Cell::Unmeasured => ("—".to_owned(), palette.muted, "Not measured yet"),
                        Cell::Done(fps) => (
                            format!("{fps:.1} fps"),
                            palette.success,
                            "Measured end to end",
                        ),
                        Cell::Failed(error) => ("failed".to_owned(), palette.error, *error),
                    };
                    egui::Frame::new()
                        .fill(tone.gamma_multiply(0.13))
                        .corner_radius(theme::control_radius())
                        .inner_margin(egui::Margin::symmetric(9, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(*resolution).size(11.0).color(palette.muted),
                                );
                                ui.label(
                                    RichText::new(value)
                                        .font(theme::title_font(11.5))
                                        .color(tone),
                                );
                            });
                        })
                        .response
                        .on_hover_text(hint);
                }
            });
        });
}

impl NativeApp {
    fn benchmark_setup(&mut self, ui: &mut egui::Ui, installed_count: usize) {
        let palette = self.palette();
        let running = self.benchmarks["status"] == "running";
        let available = self.bootstrap["system"]["available_backends"].clone();
        components::section(
            ui,
            "benchmark",
            "Run a local benchmark",
            "Measures the real pipeline on a synthetic 24 fps clip. No video is saved.",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut backend = BACKENDS
                        .iter()
                        .position(|name| *name == self.backend)
                        .unwrap_or(0);
                    if components::segmented(
                        ui,
                        "benchmark-backend",
                        &["PyTorch", "NCNN", "TensorRT"],
                        &mut backend,
                    ) && !running
                    {
                        self.backend = BACKENDS[backend].to_owned();
                        self.save_render_defaults();
                        self.get(&format!("/api/models?backend={}", self.backend));
                    }
                    let mut clip = CLIPS
                        .iter()
                        .position(|frames| *frames == self.benchmark_frames)
                        .unwrap_or(1);
                    if components::segmented(
                        ui,
                        "benchmark-clip",
                        &["1 s clip", "2 s clip", "4 s clip"],
                        &mut clip,
                    ) {
                        self.benchmark_frames = CLIPS[clip];
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if components::chip(ui, Some("all"), "All", self.benchmark_category == "all")
                        .clicked()
                    {
                        self.benchmark_category = "all".to_owned();
                    }
                    for (key, label) in CATEGORIES {
                        if components::chip(ui, Some(key), label, self.benchmark_category == key)
                            .clicked()
                        {
                            self.benchmark_category = key.to_owned();
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for resolution in RESOLUTIONS {
                        let selected = self
                            .benchmark_resolutions
                            .entry(resolution.to_owned())
                            .or_insert(false);
                        if components::chip(ui, Some("monitor"), resolution, *selected).clicked()
                            && !running
                        {
                            *selected = !*selected;
                        }
                    }
                });
                let resolutions = RESOLUTIONS
                    .iter()
                    .filter(|name| self.benchmark_resolutions.get(**name) == Some(&true))
                    .collect::<Vec<_>>();
                let ready_backend = backend_is_installed(&available, &self.backend);
                let blocker = if !self.connected {
                    Some("Waiting for the local engine.")
                } else if !ready_backend {
                    Some("This backend is not installed. Open Runtime to install it.")
                } else if installed_count == 0 {
                    Some("Download models on the Models page before benchmarking.")
                } else if resolutions.is_empty() {
                    Some("Select at least one input resolution.")
                } else {
                    None
                };
                if let Some(blocker) = blocker {
                    components::notice(ui, "info", palette.warning, blocker);
                }
                ui.horizontal_wrapped(|ui| {
                    if components::primary_button_enabled(
                        ui,
                        "Run benchmarks",
                        blocker.is_none() && !running && !self.busy,
                        210.0,
                    )
                    .clicked()
                    {
                        let categories = CATEGORIES
                            .iter()
                            .filter(|(key, _)| {
                                self.benchmark_category == "all" || self.benchmark_category == *key
                            })
                            .map(|(key, _)| *key)
                            .collect::<Vec<_>>();
                        self.post(
                            "/api/benchmarks/start",
                            json!({
                                "backend": self.backend,
                                "categories": categories,
                                "resolutions": resolutions,
                                "frames": self.benchmark_frames,
                                "precision": self.precision,
                                "pytorch_gpu_id": self.pytorch_gpu_id,
                                "ncnn_gpu_id": self.ncnn_gpu_id,
                            }),
                        );
                    }
                    if self.model_storage == "wsl"
                        && components::icon_button(ui, "refresh", "Scan Ubuntu library", true)
                            .clicked()
                    {
                        self.show_toast(
                            "Scanning WSL models",
                            "Ubuntu stays available for the benchmark and other WSL tasks.",
                            ToastKind::Info,
                        );
                        self.get(&format!(
                            "/api/models?backend={}&refresh_wsl=1",
                            self.backend
                        ));
                    }
                });
                if let Some(error) = self.benchmarks["error"].as_str().filter(|e| !e.is_empty()) {
                    components::notice(ui, "warning", palette.error, error);
                }
            },
        );
    }

    fn benchmark_chart(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let best = fastest(&self.benchmarks["results"], &self.backend, 6);
        components::section(
            ui,
            "rocket",
            "Fastest on this PC",
            "Frames per second, end to end. Longer is better.",
            |ui| {
                if best.is_empty() {
                    components::empty_state(
                        ui,
                        "interpolate",
                        "No measurements yet",
                        "Run a benchmark to compare your installed models.",
                    );
                    return;
                }
                let top = best[0].2.max(0.001);
                for (model, resolution, fps) in &best {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            Vec2::new(ui.available_width() * 0.42, 20.0),
                            egui::Label::new(format!("{model} · {resolution}")).truncate(),
                        )
                        .on_hover_text(*model);
                        let width = (ui.available_width() - 76.0).max(1.0);
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(width, 12.0), Sense::hover());
                        ui.painter().rect_filled(rect, 6.0, palette.elevated);
                        let filled = egui::Rect::from_min_size(
                            rect.min,
                            Vec2::new((rect.width() * (*fps / top) as f32).max(12.0), 12.0),
                        );
                        components::gradient_rect(
                            ui.painter(),
                            filled,
                            6.0,
                            palette.accent,
                            palette.accent_alt,
                        );
                        ui.label(
                            RichText::new(format!("{fps:.1} fps")).font(theme::title_font(12.5)),
                        );
                    });
                }
            },
        );
    }

    pub(crate) fn benchmarks_page(&mut self, ui: &mut egui::Ui) {
        let categories = CATEGORIES;
        let installed_count = categories
            .iter()
            .map(|(category, _)| {
                self.models["model_status"][*category]
                    .as_object()
                    .map_or(0, |models| {
                        models
                            .values()
                            .filter(|installed| installed.as_bool() == Some(true))
                            .count()
                    })
            })
            .sum::<usize>();
        self.benchmark_setup(ui, installed_count);
        ui.add_space(theme::space::MD);
        self.benchmark_chart(ui);
        ui.add_space(theme::space::MD);
        self.benchmark_results(ui);
        ui.add_space(theme::space::MD);
        self.benchmark_hardware(ui, installed_count);
    }

    /// Every catalogue model, grouped by purpose. It grows with the page; a
    /// second scroll box here would be squeezed to whatever height was left.
    fn benchmark_results(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::section(
            ui,
            "database",
            "All results",
            "Every catalogue model. Measurements are taken and saved on this PC.",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.add_sized(
                        Vec2::new(ui.available_width().min(340.0), 36.0),
                        egui::TextEdit::singleline(&mut self.benchmark_search)
                            .margin(Vec2::new(12.0, 9.0))
                            .hint_text("Filter model names…"),
                    );
                    if components::chip(
                        ui,
                        Some("check"),
                        "Installed only",
                        self.benchmark_installed_only,
                    )
                    .clicked()
                    {
                        self.benchmark_installed_only = !self.benchmark_installed_only;
                    }
                });
                self.benchmark_failures(ui);

                let query = self.benchmark_search.trim().to_lowercase();
                let mut shown = 0;
                for (category, title) in CATEGORIES {
                    if self.benchmark_category != "all" && self.benchmark_category != category {
                        continue;
                    }
                    let installed = |name: &str| {
                        self.models["model_status"][category][name].as_bool() == Some(true)
                    };
                    let all = self.models[category]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>();
                    let rows = all
                        .iter()
                        .copied()
                        .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
                        .filter(|name| !self.benchmark_installed_only || installed(name))
                        .collect::<Vec<_>>();
                    if rows.is_empty() {
                        continue;
                    }
                    shown += 1;
                    let ready = all.iter().filter(|name| installed(name)).count();
                    // Searching or narrowing to one purpose opens the group.
                    let forced = !query.is_empty() || self.benchmark_category != "all";
                    category_group(ui, category, title, ready, all.len(), forced, |ui| {
                        for name in &rows {
                            model_row(
                                ui,
                                name,
                                installed(name),
                                &RESOLUTIONS.map(|resolution| {
                                    (
                                        resolution,
                                        cell(
                                            &self.benchmarks["results"],
                                            &self.backend,
                                            category,
                                            name,
                                            resolution,
                                        ),
                                    )
                                }),
                            );
                        }
                    });
                }
                if shown == 0 {
                    components::empty_state(
                        ui,
                        "denoise",
                        "No models to show",
                        if !self.connected {
                            "The list appears when the local engine connects."
                        } else if self.benchmark_installed_only {
                            "Nothing installed matches. Turn off “Installed only”."
                        } else {
                            "Try a different name or purpose."
                        },
                    );
                }
                ui.label(
                    RichText::new(
                        "Scene-change detectors run automatically during interpolation, so they are not measured on their own.",
                    )
                    .size(11.0)
                    .color(palette.muted),
                );
            },
        );
    }

    fn benchmark_failures(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let failures = self.benchmarks["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["backend"] == self.backend.as_str() && item["status"] == "error")
            .collect::<Vec<_>>();
        if failures.is_empty() {
            return;
        }
        components::notice(
            ui,
            "warning",
            palette.error,
            &format!("{} benchmark(s) failed on this backend.", failures.len()),
        );
        egui::CollapsingHeader::new("Show error details")
            .id_salt("benchmark-error-details")
            .show(ui, |ui| {
                for result in failures.iter().rev().take(20) {
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            result["model"].as_str().unwrap_or("Unknown model"),
                            result["resolution"].as_str().unwrap_or("")
                        ))
                        .font(theme::title_font(12.5)),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                result["error"]
                                    .as_str()
                                    .unwrap_or("The benchmark failed without a message."),
                            )
                            .size(11.0)
                            .color(palette.error),
                        )
                        .wrap(),
                    );
                }
            });
    }

    fn benchmark_hardware(&mut self, ui: &mut egui::Ui, installed_count: usize) {
        let palette = self.palette();
        let system = &self.bootstrap["system"];
        let text = |key: &str| system[key].as_str().unwrap_or("Not detected").to_owned();
        let tiles = [
            ("gpu", "Graphics", text("gpu_name"), palette.success),
            ("chip", "Processor", text("cpu"), palette.accent),
            ("memory", "Memory", text("ram"), palette.accent_alt),
            (
                "package",
                "Installed models",
                installed_count.to_string(),
                palette.warning,
            ),
        ];
        let columns = if ui.available_width() >= 760.0 { 2 } else { 1 };
        for row in tiles.chunks(columns) {
            ui.columns(columns, |cells| {
                for (cell, (icon, label, value, tone)) in cells.iter_mut().zip(row) {
                    components::stat(cell, icon, *tone, label, value);
                }
            });
            ui.add_space(theme::space::XS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, cell, fastest};
    use serde_json::json;

    #[test]
    fn cell_uses_the_newest_matching_result() {
        let entry = |status: &str, resolution: &str, fps: f64| {
            json!({"backend": "pytorch", "category": "upscale", "model": "m",
                   "resolution": resolution, "status": status, "measured_fps": fps, "error": "boom"})
        };
        let results = json!([
            entry("complete", "720p", 5.0),
            entry("complete", "720p", 9.0),
            entry("error", "4K", 0.0)
        ]);
        let at = |resolution| cell(&results, "pytorch", "upscale", "m", resolution);
        assert_eq!(at("720p"), Cell::Done(9.0));
        assert_eq!(at("4K"), Cell::Failed("boom"));
        assert_eq!(at("1080p"), Cell::Unmeasured);
        assert_eq!(
            cell(&results, "ncnn", "upscale", "m", "720p"),
            Cell::Unmeasured
        );
    }

    #[test]
    fn fastest_keeps_only_completed_results_of_one_backend_in_order() {
        let results = json!([
            {"backend": "pytorch", "status": "complete", "model": "slow", "resolution": "720p", "measured_fps": 4.0},
            {"backend": "pytorch", "status": "error", "model": "bad", "resolution": "720p", "measured_fps": 99.0},
            {"backend": "ncnn", "status": "complete", "model": "other", "resolution": "720p", "measured_fps": 50.0},
            {"backend": "pytorch", "status": "complete", "model": "fast", "resolution": "720p", "measured_fps": 30.0}
        ]);
        let names = fastest(&results, "pytorch", 5)
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>();
        assert_eq!(names, ["fast", "slow"]);
    }
}
