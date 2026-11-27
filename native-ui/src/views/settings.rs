//! Settings, split into tabs: appearance, rendering, storage, updates, about.

use eframe::egui::{self, Color32, Rect, RichText, Sense, Vec2};
use serde_json::{Value, json};

use crate::{
    APP_VERSION, NativeApp, format_bytes, settings_uses_two_columns,
    ui::{components, icons, theme},
    update_ui_setting,
};

const TABS: [(&str, &str); 5] = [
    ("palette", "Appearance"),
    ("sliders", "Rendering"),
    ("storage", "Storage"),
    ("update", "Updates"),
    ("info", "About"),
];
const CHART_COLORS: [Color32; 7] = [
    Color32::from_rgb(88, 142, 248),
    Color32::from_rgb(160, 112, 246),
    Color32::from_rgb(44, 190, 152),
    Color32::from_rgb(240, 148, 60),
    Color32::from_rgb(232, 90, 146),
    Color32::from_rgb(222, 180, 62),
    Color32::from_rgb(120, 200, 230),
];

/// A switch used as the control of a settings row.
fn switch(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    let accent = components::palette_of(ui).accent;
    let still = ui.style().animation_time == 0.0;
    components::toggle(ui, value, label, accent, still).changed()
}

impl NativeApp {
    pub(crate) fn settings_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (index, (icon, label)) in TABS.iter().enumerate() {
                if components::chip(ui, Some(icon), label, self.settings_tab == index).clicked() {
                    self.settings_tab = index;
                }
            }
        });
        ui.add_space(theme::space::MD);
        match self.settings_tab {
            0 => self.appearance_settings(ui),
            1 => self.rendering_settings(ui),
            2 => self.storage_settings(ui),
            3 => self.update_settings(ui),
            _ => self.about_settings(ui),
        }
    }

    fn rendering_settings(&mut self, ui: &mut egui::Ui) {
        let mut tuned = false;
        let mut switched = false;
        let mut preview = self.saved_settings["advanced"]["preview_enabled"]
            .as_bool()
            .unwrap_or(true);
        let two = settings_uses_two_columns(ui.available_width());
        let mut numbers = |app: &mut Self, ui: &mut egui::Ui| {
            components::section(
                ui,
                "sliders",
                "Performance",
                "Defaults used by every new render.",
                |ui| {
                    components::row(
                        ui,
                        "target",
                        "Precision",
                        "float16 is faster on modern GPUs.",
                        |ui| {
                            egui::ComboBox::from_id_salt("default-precision")
                                .selected_text(&app.precision)
                                .show_ui(ui, |ui| {
                                    for option in ["auto", "float16", "float32"] {
                                        tuned |= ui
                                            .selectable_value(
                                                &mut app.precision,
                                                option.to_owned(),
                                                option,
                                            )
                                            .changed();
                                    }
                                });
                        },
                    );
                    for (icon, title, detail, value, limit) in [
                        (
                            "grid",
                            "Tile size",
                            "0 picks automatically. Smaller uses less VRAM.",
                            &mut app.tile_size,
                            4096,
                        ),
                        (
                            "layers",
                            "Tile overlap",
                            "Hides seams between tiles.",
                            &mut app.overlap,
                            256,
                        ),
                        (
                            "gpu",
                            "PyTorch GPU",
                            "Index of the graphics card.",
                            &mut app.pytorch_gpu_id,
                            64,
                        ),
                        (
                            "chip",
                            "NCNN GPU",
                            "Index of the Vulkan device.",
                            &mut app.ncnn_gpu_id,
                            64,
                        ),
                    ] {
                        components::row(ui, icon, title, detail, |ui| {
                            tuned |= ui
                                .add(egui::DragValue::new(value).range(0..=limit))
                                .changed();
                        });
                    }
                },
            );
        };
        let mut switches = |app: &mut Self, ui: &mut egui::Ui| {
            components::section(
                ui,
                "film",
                "Output behaviour",
                "What happens to the picture and the file.",
                |ui| {
                    for (icon, title, detail, value) in [
                        (
                            "sun",
                            "Automatic HDR",
                            "Keep HDR sources in HDR.",
                            &mut app.auto_hdr,
                        ),
                        (
                            "monitor",
                            "UHD mode",
                            "Faster interpolation for very large video.",
                            &mut app.uhd_mode,
                        ),
                        (
                            "crop",
                            "Detect black borders",
                            "Crop bars before processing.",
                            &mut app.auto_crop,
                        ),
                        (
                            "save",
                            "Replace existing output",
                            "Overwrite a file with the same name.",
                            &mut app.overwrite,
                        ),
                        (
                            "play",
                            "Send output to MPV",
                            "Play instead of saving a file.",
                            &mut app.output_to_mpv,
                        ),
                        (
                            "eye",
                            "Live render preview",
                            "Show a real frame while rendering.",
                            &mut preview,
                        ),
                    ] {
                        components::row(ui, icon, title, detail, |ui| {
                            switched |= switch(ui, value, title);
                        });
                    }
                },
            );
        };
        if two {
            ui.columns(2, |columns| {
                numbers(self, &mut columns[0]);
                switches(self, &mut columns[1]);
            });
        } else {
            numbers(self, ui);
            ui.add_space(theme::space::MD);
            switches(self, ui);
        }
        if tuned || switched {
            if !self.saved_settings["advanced"].is_object() {
                self.saved_settings["advanced"] = json!({});
            }
            self.saved_settings["advanced"]["preview_enabled"] = json!(preview);
            self.save_render_defaults();
        }
    }

    fn storage_settings(&mut self, ui: &mut egui::Ui) {
        if settings_uses_two_columns(ui.available_width()) {
            ui.columns(2, |columns| {
                self.model_storage_section(&mut columns[0]);
                columns[0].add_space(theme::space::MD);
                self.cleanup_section(&mut columns[0]);
                self.footprint_section(&mut columns[1]);
            });
        } else {
            self.model_storage_section(ui);
            ui.add_space(theme::space::MD);
            self.footprint_section(ui);
            ui.add_space(theme::space::MD);
            self.cleanup_section(ui);
        }
    }

    fn model_storage_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::section(
            ui,
            "database",
            "Model storage",
            "Where model weights are installed and read from.",
            |ui| {
                let mut index = usize::from(self.model_storage != "wsl");
                if components::segmented(
                    ui,
                    "storage-location",
                    &["WSL Ubuntu", "This PC · Windows"],
                    &mut index,
                ) {
                    self.model_storage = ["wsl", "windows"][index].to_owned();
                }
                if self.first_setup {
                    components::notice(
                        ui,
                        "rocket",
                        palette.warning,
                        "First launch: choose where to keep the models, then start setup.",
                    );
                }
                let status = self.storage_status["status"].as_str().unwrap_or("waiting");
                let stage = self.storage_status["stage"]
                    .as_str()
                    .unwrap_or("Waiting for engine status");
                if status == "ready" {
                    components::notice(ui, "check", palette.success, "Model storage is ready.");
                } else {
                    components::notice(ui, "info", palette.accent, stage);
                }
                if let Some(error) = self.storage_status["error"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                {
                    components::notice(ui, "warning", palette.error, error);
                }
                components::copyable_path(
                    ui,
                    self.storage_status["model_dir"]
                        .as_str()
                        .unwrap_or_default(),
                    "Storage path appears after setup",
                );
                if components::primary_button_enabled(
                    ui,
                    "Apply and verify all models",
                    self.benchmarks["status"] != "running" && self.connected,
                    260.0,
                )
                .clicked()
                {
                    update_ui_setting("model_storage", json!(self.model_storage));
                    self.first_setup = false;
                    self.post(
                        "/api/storage/configure",
                        json!({"storage": self.model_storage}),
                    );
                }
            },
        );
    }

    fn footprint_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        if self.footprint.is_null() && self.connected && !self.footprint_loading {
            self.footprint_loading = true;
            self.get("/api/storage/footprint");
        }
        components::section(
            ui,
            "storage",
            "Disk usage",
            "What the app keeps on this PC and in WSL.",
            |ui| {
                let parts = self.footprint["components"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                Some((
                                    item["label"].as_str()?,
                                    item["bytes"].as_u64()?,
                                    item["storage"] == "wsl",
                                ))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let total: u64 = parts.iter().map(|part| part.1).sum();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if self.footprint.is_null() {
                            "—".to_owned()
                        } else {
                            format_bytes(total)
                        })
                        .font(theme::title_font(30.0))
                        .color(palette.text),
                    );
                    ui.vertical(|ui| {
                        ui.label(RichText::new("managed").size(11.5).color(palette.muted));
                        if let Some(free) = self.footprint["free_bytes"].as_u64() {
                            ui.label(
                                RichText::new(format!("{} free", format_bytes(free)))
                                    .size(11.5)
                                    .color(palette.secondary),
                            );
                        }
                    });
                });
                usage_bar(ui, &parts, total);
                for (index, (label, bytes, wsl)) in parts.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                        ui.painter()
                            .circle_filled(dot.center(), 4.5, CHART_COLORS[index % 7]);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format_bytes(*bytes)).font(theme::title_font(12.5)),
                            );
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let name = if *wsl {
                                        format!("{label} (WSL)")
                                    } else {
                                        (*label).to_owned()
                                    };
                                    ui.add(egui::Label::new(name).truncate());
                                },
                            );
                        });
                    });
                }
                if let Some(error) = self.footprint["error"].as_str().filter(|e| !e.is_empty()) {
                    components::notice(ui, "warning", palette.warning, error);
                }
                if self.model_storage == "wsl" && self.footprint["wsl_size_known"] == false {
                    components::notice(
                        ui,
                        "info",
                        palette.accent,
                        "Ubuntu is stopped, so its model size has not been measured. Checking sizes never starts WSL.",
                    );
                }
                let reclaimable = self.footprint["reclaimable_bytes"].as_u64().unwrap_or(0);
                if reclaimable > 0 {
                    components::notice(
                        ui,
                        "zap",
                        palette.success,
                        &format!(
                            "{} of build-only PyTorch files can be freed.",
                            format_bytes(reclaimable)
                        ),
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    let idle = self.connected && self.active_task().is_none();
                    if reclaimable > 0
                        && components::primary_button_enabled(ui, "Slim runtime", idle, 170.0)
                            .clicked()
                    {
                        self.destructive_confirmation = Some("slim".to_owned());
                        self.footprint = Value::Null;
                    }
                    if components::icon_button(
                        ui,
                        "refresh",
                        "Refresh sizes",
                        !self.footprint_loading,
                    )
                    .clicked()
                    {
                        self.footprint_loading = true;
                        self.get("/api/storage/footprint");
                    }
                });
            },
        );
    }

    fn cleanup_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let cache = self.footprint["components"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == "uv_cache"))
            .and_then(|item| item["bytes"].as_u64())
            .unwrap_or(0);
        components::section(
            ui,
            "trash",
            "Free up space",
            "Your videos and the app itself are never touched.",
            |ui| {
                let idle = self.active_task().is_none() && !self.managed_data_pending;
                let cache_detail = format!("{} of cached package downloads.", format_bytes(cache));
                for (icon, title, detail, kind, enabled) in [
                    (
                        "package",
                        "Package cache",
                        cache_detail.as_str(),
                        "uv_cache",
                        idle && cache > 0,
                    ),
                    (
                        "python",
                        "Python and packages",
                        "Runtime setup can install them again.",
                        "python",
                        idle,
                    ),
                    (
                        "models",
                        "All models",
                        "Includes custom imports.",
                        "models",
                        idle,
                    ),
                ] {
                    components::row(ui, icon, title, detail, |ui| {
                        let label = if kind == "uv_cache" {
                            "Prune"
                        } else {
                            "Delete"
                        };
                        if components::icon_button(ui, "trash", label, enabled).clicked() {
                            self.managed_data_message = None;
                            self.destructive_confirmation = Some(kind.to_owned());
                        }
                    });
                }
                if let Some(message) = self.managed_data_message.as_deref() {
                    let tone = if self.managed_data_failed {
                        palette.error
                    } else if self.managed_data_pending {
                        palette.accent
                    } else {
                        palette.success
                    };
                    components::notice(ui, "info", tone, message);
                }
            },
        );
    }

    fn update_settings(&mut self, ui: &mut egui::Ui) {
        let two = settings_uses_two_columns(ui.available_width());
        let status = self.update["status"].as_str().unwrap_or("idle").to_owned();
        let release = |app: &mut Self, ui: &mut egui::Ui| {
            let palette = app.palette();
            components::section(
                ui,
                "update",
                "App updates",
                "Releases come from B1progame on GitHub.",
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("v{APP_VERSION}"))
                                .font(theme::title_font(30.0))
                                .color(palette.text),
                        );
                        if let Some(latest) = app.update["latest_version"].as_str() {
                            components::status_badge(
                                ui,
                                &format!("latest v{latest}"),
                                if app.update["update_available"] == true {
                                    palette.warning
                                } else {
                                    palette.success
                                },
                            );
                        }
                    });
                    if let Some(detail) = app.update["detail"].as_str() {
                        let tone = if status == "error" {
                            palette.error
                        } else {
                            palette.accent
                        };
                        components::notice(ui, "info", tone, detail);
                    }
                    ui.horizontal_wrapped(|ui| {
                        let checking = status == "checking";
                        if components::primary_button_enabled(
                            ui,
                            if checking {
                                "Checking…"
                            } else {
                                "Check for updates"
                            },
                            !checking && status != "downloading",
                            200.0,
                        )
                        .clicked()
                        {
                            app.update = json!({
                                "status": "checking",
                                "current_version": APP_VERSION,
                                "detail": "Checking the B1progame GitHub releases…"
                            });
                            app.get("/api/app/update/check");
                        }
                        if app.update["update_available"] == true
                            && !matches!(
                                status.as_str(),
                                "downloading" | "downloaded" | "installing"
                            )
                            && components::icon_button(
                                ui,
                                "download",
                                "Download and install",
                                !app.other_task_active(),
                            )
                            .clicked()
                        {
                            app.post("/api/app/update/download", json!({}));
                        }
                    });
                    ui.hyperlink_to(
                        "All releases ↗",
                        "https://github.com/B1progame/REAL-Video-Enhancer/releases",
                    );
                    if let Some(url) = app.update["release_url"].as_str() {
                        ui.hyperlink_to("Release notes ↗", url);
                    }
                },
            );
        };
        let access = |app: &mut Self, ui: &mut egui::Ui| {
            let palette = app.palette();
            components::section(
                ui,
                "key",
                "Private repository access",
                "Optional. Public releases need no sign-in.",
                |ui| {
                    let saved = app.update["has_github_token"] == true;
                    components::notice(
                        ui,
                        "shield",
                        if saved {
                            palette.success
                        } else {
                            palette.accent
                        },
                        if saved {
                            "A token is saved, encrypted for this Windows account."
                        } else {
                            "The app uses a saved token or your GitHub CLI login. Tokens are encrypted for this Windows account."
                        },
                    );
                    ui.add_sized(
                        Vec2::new(ui.available_width(), 36.0),
                        egui::TextEdit::singleline(&mut app.update_token_input)
                            .password(true)
                            .margin(Vec2::new(12.0, 9.0))
                            .hint_text("github_pat_… · read-only Contents access"),
                    );
                    ui.horizontal_wrapped(|ui| {
                        let typed = !app.update_token_input.trim().is_empty();
                        if components::icon_button(ui, "save", "Save token", typed).clicked() {
                            let token = std::mem::take(&mut app.update_token_input);
                            app.post("/api/app/update/auth", json!({"token": token}));
                        }
                        if saved
                            && components::icon_button(ui, "trash", "Remove token", true).clicked()
                        {
                            app.post("/api/app/update/auth", json!({"token": ""}));
                        }
                        ui.hyperlink_to(
                            "Create a read-only token ↗",
                            "https://github.com/settings/personal-access-tokens/new",
                        );
                    });
                    if !app.update_auth_message.is_empty() {
                        components::notice(ui, "info", palette.accent, &app.update_auth_message);
                    }
                },
            );
        };
        if two {
            ui.columns(2, |columns| {
                release(self, &mut columns[0]);
                access(self, &mut columns[1]);
            });
        } else {
            release(self, ui);
            ui.add_space(theme::space::MD);
            access(self, ui);
        }
    }

    fn about_settings(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::glass_frame(palette).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(theme::space::MD);
                self.logo(ui, 104.0);
                ui.label(
                    RichText::new("REAL Video Enhancer")
                        .font(theme::title_font(24.0))
                        .color(palette.text),
                );
                ui.label(
                    RichText::new(format!("Version {APP_VERSION} · AGPL-3.0 · runs locally"))
                        .color(palette.secondary),
                );
                ui.add_space(theme::space::SM);
                ui.hyperlink_to(
                    "Source on GitHub ↗",
                    "https://github.com/B1progame/REAL-Video-Enhancer",
                );
                ui.hyperlink_to(
                    "Community models on OpenModelDB ↗",
                    "https://openmodeldb.info",
                );
                ui.add_space(theme::space::MD);
            });
        });
        ui.add_space(theme::space::MD);
        components::section(
            ui,
            "command",
            "Keyboard shortcuts",
            "Everything is also reachable by mouse.",
            |ui| {
                for (keys, action) in [
                    ("Ctrl K", "Command palette"),
                    ("Ctrl 1 – 8", "Switch page"),
                    ("Drag and drop", "Open a video; extra files join the queue"),
                    ("Space", "Flip the focused switch"),
                ] {
                    components::row(ui, "zap", action, "", |ui| {
                        components::metadata_chip(ui, keys, palette.accent);
                    });
                }
                ui.horizontal(|ui| {
                    ui.add(icons::styled("shield", palette.success, 16.0));
                    ui.label("Your footage never leaves this PC.");
                });
            },
        );
    }
}

/// One stacked bar showing each part's share.
fn usage_bar(ui: &mut egui::Ui, parts: &[(&str, u64, bool)], total: u64) {
    let palette = components::palette_of(ui);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(1.0), 14.0),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 7.0, palette.elevated);
    if total == 0 {
        return;
    }
    let mut left = rect.left();
    for (index, (_, bytes, _)) in parts.iter().enumerate() {
        let width = rect.width() * (*bytes as f64 / total as f64) as f32;
        if width >= 1.0 {
            let segment = Rect::from_min_size(egui::pos2(left, rect.top()), Vec2::new(width, 14.0));
            ui.painter().rect_filled(
                segment.shrink2(Vec2::new(0.5, 0.0)),
                4.0,
                CHART_COLORS[index % 7],
            );
        }
        left += width;
    }
}
