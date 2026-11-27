//! Local model benchmarks: setup, the fastest results, and the full table.

use eframe::egui::{self, RichText, Sense, Vec2};
use serde_json::{Value, json};

use crate::{
    NativeApp, ToastKind, backend_is_installed,
    ui::{components, icons, theme},
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
        let palette = self.palette();
        let categories = CATEGORIES;
        let resolutions = RESOLUTIONS;
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
        components::card(ui, "All results", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Every catalog model is listed. Results are local measurements, saved on this PC.").color(palette.secondary));
                ui.add_space(theme::space::MD);
                ui.add(icons::styled("search", palette.muted, 16.0));
                ui.add_sized(Vec2::new(ui.available_width().min(280.0), 36.0),
                    egui::TextEdit::singleline(&mut self.benchmark_search).hint_text("Filter model names…"));
            });
            let failures = self.benchmarks["results"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter(|item| {
                            item["backend"].as_str() == Some(self.backend.as_str())
                                && item["status"] == "error"
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !failures.is_empty() {
                ui.add_space(theme::space::SM);
                egui::CollapsingHeader::new(format!(
                    "Benchmark errors · {} details",
                    failures.len()
                ))
                .id_salt("benchmark-error-details")
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(190.0)
                        .show(ui, |ui| {
                            for result in failures.iter().rev().take(40) {
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {} · {}",
                                        result["category"].as_str().unwrap_or("model"),
                                        result["model"].as_str().unwrap_or("Unknown model"),
                                        result["resolution"].as_str().unwrap_or("resolution")
                                    ))
                                    .strong(),
                                );
                                ui.label(
                                    RichText::new(result["error"].as_str().unwrap_or(
                                        "The benchmark failed without an error message.",
                                    ))
                                    .size(11.0)
                                    .color(palette.error),
                                );
                                ui.add_space(theme::space::XS);
                            }
                            if failures.len() > 40 {
                                ui.label(
                                    RichText::new("Showing the latest 40 errors.")
                                        .size(11.0)
                                        .color(palette.muted),
                                );
                            }
                        });
                });
            }
            ui.add_space(theme::space::SM);
            egui::ScrollArea::vertical().max_height(650.0).show(ui, |ui| {
                for (category, title) in categories {
                    if self.benchmark_category != "all" && self.benchmark_category != category { continue; }
                    let Some(names) = self.models[category].as_array() else { continue; };
                    let matching = names.iter().filter_map(Value::as_str)
                        .filter(|name| self.benchmark_search.trim().is_empty() || name.to_lowercase().contains(&self.benchmark_search.trim().to_lowercase()))
                        .collect::<Vec<_>>();
                    if matching.is_empty() { continue; }
                    let ready_count = matching.iter().filter(|name| self.models["model_status"][category][**name].as_bool() == Some(true)).count();
                    egui::CollapsingHeader::new(format!("{title} · {ready_count}/{} installed", matching.len()))
                        .id_salt(("benchmark-category", category)).default_open(self.benchmark_category != "all")
                        .show(ui, |ui| {
                            egui::ScrollArea::horizontal().id_salt(("benchmark-table", category)).show(ui, |ui| {
                                ui.set_min_width(790.0_f32.max(ui.available_width()));
                                egui::Grid::new(("benchmark-grid", category)).num_columns(7).striped(true)
                                    .min_col_width(76.0).show(ui, |ui| {
                                        ui.add_sized(Vec2::new(260.0, 28.0), egui::Label::new(RichText::new("Model").strong()));
                                        ui.label(RichText::new("Status").strong());
                                        for resolution in resolutions { ui.label(RichText::new(resolution).strong()); }
                                        ui.end_row();
                                        for name in &matching {
                                            let name = *name;
                                            let installed = self.models["model_status"][category][name].as_bool() == Some(true);
                                            ui.add_sized(Vec2::new(260.0, 32.0), egui::Label::new(RichText::new(name).size(12.0)).truncate()).on_hover_text(name);
                                            components::status_dot(ui, if installed { palette.success } else { palette.muted }, if installed { "Installed" } else { "Not installed" });
                                            for resolution in resolutions {
                                                let result = self.benchmarks["results"].as_array().and_then(|items| items.iter().rev().find(|item| {
                                                    item["backend"].as_str() == Some(self.backend.as_str())
                                                        && item["category"].as_str() == Some(category)
                                                        && item["model"].as_str() == Some(name)
                                                        && item["resolution"].as_str() == Some(resolution)
                                                }));
                                                if let Some(result) = result.filter(|_| installed) {
                                                    if result["status"] == "complete" {
                                                        let fps = result["measured_fps"].as_f64().unwrap_or(0.0);
                                                        let seconds = result["seconds"].as_f64().unwrap_or(0.0);
                                                        ui.label(RichText::new(format!("{fps:.1} fps")).color(palette.success)).on_hover_text(format!("{seconds:.2} sec for {} frames · {} precision · GPU {} · {} px tiles · end-to-end", result["frames"].as_u64().unwrap_or(0), result["precision"].as_str().unwrap_or("auto"), result["gpu_id"].as_u64().unwrap_or(0), result["tile_size"].as_u64().unwrap_or(256)));
                                                    } else if result["status"] == "error" {
                                                        ui.colored_label(palette.error, "Failed").on_hover_text(result["error"].as_str().unwrap_or("Benchmark failed."));
                                                    } else {
                                                        ui.label(RichText::new("—").color(palette.muted));
                                                    }
                                                } else if installed {
                                                    ui.label(RichText::new("—").color(palette.muted)).on_hover_text("Not measured on this PC yet.");
                                                } else {
                                                    ui.label(RichText::new("—").color(palette.muted));
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                        });
                    ui.add_space(theme::space::XS);
                }
                ui.label(RichText::new("Scene-change detectors are evaluated automatically during interpolation and are not standalone render models.").size(11.0).color(palette.muted));
            });
        });

        ui.add_space(theme::space::SM);
        let system = &self.bootstrap["system"];
        ui.label(
            RichText::new(format!(
                "{} · {} · {} RAM · {installed_count} installed models",
                system["gpu_name"].as_str().unwrap_or("GPU not detected"),
                system["cpu"].as_str().unwrap_or("CPU unavailable"),
                system["ram"].as_str().unwrap_or("unknown"),
            ))
            .size(11.0)
            .color(palette.secondary),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::fastest;
    use serde_json::json;

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
