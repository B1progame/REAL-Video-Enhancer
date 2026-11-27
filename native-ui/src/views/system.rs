//! Runtime setup and the managed Python environment.

use eframe::egui::{self, RichText};
use serde_json::{Value, json};

use crate::{
    NativeApp, RUNTIME_PAGE, backend_is_installed, runtime_metrics_use_two_columns,
    ui::{art, components, theme},
};

/// Key, name, picture, what it is for.
const BACKENDS: [(&str, &str, &str, &str); 3] = [
    (
        "pytorch",
        "PyTorch",
        "upscale",
        "Most models. Best on NVIDIA with CUDA.",
    ),
    (
        "ncnn",
        "NCNN",
        "interpolate",
        "Light Vulkan engine for AMD, Intel and NVIDIA.",
    ),
    (
        "tensorrt",
        "TensorRT",
        "deblur",
        "Fastest on NVIDIA after a one-time model build.",
    ),
];

impl NativeApp {
    pub(crate) fn runtime_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let status = self.install["status"]
            .as_str()
            .unwrap_or("not_started")
            .to_owned();
        let installing = status == "running";
        let enabled = !installing && !self.busy && self.benchmarks["status"] != "running";
        let available = self.bootstrap["system"]["available_backends"].clone();
        let any = available.as_array().is_some_and(|list| !list.is_empty());
        let ready = any && self.python["python_managed"] == true;

        components::section(
            ui,
            "rocket",
            "Inference runtime",
            "One click installs everything needed to render.",
            |ui| {
                let (icon, tone, message) = if installing {
                    (
                        "wrench",
                        palette.accent,
                        "Setup is running. Progress is shown above.",
                    )
                } else if status == "error" {
                    ("warning", palette.error, "The last setup failed.")
                } else if status == "cancelled" {
                    ("warning", palette.warning, "The last setup was cancelled.")
                } else if ready {
                    ("check", palette.success, "The runtime is ready to render.")
                } else {
                    (
                        "info",
                        palette.warning,
                        "Install a backend to enable enhancement.",
                    )
                };
                components::notice(
                    ui,
                    icon,
                    tone,
                    self.install["detail"]
                        .as_str()
                        .filter(|_| status != "not_started")
                        .unwrap_or(message),
                );
                if let Some(issue) = self.bootstrap["system"]["backend_probe_output"]
                    .as_str()
                    .filter(|issue| issue.starts_with("TensorRT package versions do not match:"))
                {
                    components::notice(
                        ui,
                        "warning",
                        palette.warning,
                        &format!(
                            "{issue} Run the recommended setup to restore PyTorch 2.8, then install TensorRT again."
                        ),
                    );
                }
                if components::primary_button_enabled(
                    ui,
                    "Install recommended runtime",
                    enabled,
                    280.0,
                )
                .clicked()
                {
                    self.start_recommended_runtime_setup();
                }
            },
        );

        ui.add_space(theme::space::MD);
        let wide = ui.available_width() >= 720.0;
        let mut install = None;
        let mut backend_card = |ui: &mut egui::Ui, index: usize| {
            let (key, name, picture, detail) = BACKENDS[index];
            let present = backend_is_installed(&available, key);
            let tone = [palette.accent, palette.accent_alt, palette.warning][index];
            components::glass_frame(palette)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    art::banner(ui, picture, tone, 70.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(name)
                                .font(theme::title_font(15.0))
                                .color(palette.text),
                        );
                        components::status_badge(
                            ui,
                            if present {
                                "Installed"
                            } else {
                                "Not installed"
                            },
                            if present {
                                palette.success
                            } else {
                                palette.muted
                            },
                        );
                    });
                    ui.add(
                        egui::Label::new(RichText::new(detail).size(11.5).color(palette.secondary))
                            .wrap(),
                    );
                    let label = if present { "Reinstall" } else { "Install" };
                    if components::icon_button(ui, "download", label, enabled).clicked() {
                        install = Some(key);
                    }
                });
        };
        if wide {
            ui.columns(3, |columns| {
                for (index, column) in columns.iter_mut().enumerate() {
                    backend_card(column, index);
                }
            });
        } else {
            for index in 0..BACKENDS.len() {
                backend_card(ui, index);
                ui.add_space(theme::space::SM);
            }
        }
        if let Some(backend) = install {
            self.post(
                "/api/actions/manage_backend",
                json!({"action": "install", "backend": backend}),
            );
        }

        ui.add_space(theme::space::MD);
        self.hardware_tiles(ui);

        if let Some(lines) = self.install["output"]
            .as_array()
            .filter(|lines| !lines.is_empty())
        {
            ui.add_space(theme::space::MD);
            components::section(
                ui,
                "terminal",
                "Setup output",
                "The most recent lines from the installer.",
                |ui| {
                    components::terminal(
                        ui,
                        "runtime-output",
                        lines.iter().rev().take(120).rev().filter_map(Value::as_str),
                        260.0,
                    );
                },
            );
        }

        ui.add_space(theme::space::MD);
        self.animescripter_section(ui);
    }

    fn hardware_tiles(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let system = self.bootstrap["system"].clone();
        let value = |key: &str| system[key].as_str().unwrap_or("Not detected").to_owned();
        let tiles = [
            ("monitor", "Operating system", value("os"), palette.accent),
            ("gpu", "Graphics", value("gpu_name"), palette.success),
            ("zap", "CUDA", value("cuda_version"), palette.warning),
            ("memory", "Memory", value("ram"), palette.accent_alt),
        ];
        if runtime_metrics_use_two_columns(ui.available_width()) {
            for pair in tiles.as_chunks::<2>().0 {
                ui.columns(2, |columns| {
                    for (column, (icon, label, value, tone)) in columns.iter_mut().zip(pair) {
                        components::stat(column, icon, *tone, label, value);
                    }
                });
                ui.add_space(theme::space::SM);
            }
        } else {
            for (icon, label, value, tone) in &tiles {
                components::stat(ui, icon, *tone, label, value);
            }
        }
        if components::icon_button(ui, "refresh", "Detect hardware again", self.connected).clicked()
        {
            self.post("/api/actions/refresh_backends", json!({}));
            self.get("/api/bootstrap");
        }
    }

    fn animescripter_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::section(
            ui,
            "sparkles",
            "AnimeScripter",
            "Optional anime-focused engine. Pick it in the Studio.",
            |ui| {
                let ready = self.anime["ready"] == true;
                components::notice(
                    ui,
                    if ready { "check" } else { "info" },
                    if ready {
                        palette.success
                    } else {
                        palette.accent
                    },
                    self.anime["detail"]
                        .as_str()
                        .filter(|detail| !detail.is_empty())
                        .unwrap_or("Not installed."),
                );
                if let Some(path) = self.anime["executable"].as_str() {
                    components::copyable_path(ui, path, "AnimeScripter CLI path");
                }
                ui.horizontal_wrapped(|ui| {
                    let idle = self.connected && self.active_task().is_none();
                    let label = if ready { "Repair" } else { "Install" };
                    if components::icon_button(ui, "download", label, idle).clicked() {
                        self.post("/api/animescripter/install", json!({}));
                    }
                    ui.hyperlink_to(
                        "Official project ↗",
                        "https://github.com/NevermindNilas/TheAnimeScripter",
                    );
                });
            },
        );
    }

    pub(crate) fn python_page(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let pending = self.connected && self.python.is_null();
        let installing = self.install["status"] == "running";
        let present = self.python["python_present"] == true;
        let managed = self.python["python_managed"] == true;
        let failed = self.install["status"] == "error";
        let path = self.python["python_path"].as_str().unwrap_or("").to_owned();

        components::section(
            ui,
            "python",
            "Managed Python",
            "The app keeps its own interpreter, separate from your system.",
            |ui| {
                let (icon, tone, title) = if managed {
                    ("check", palette.success, "Interpreter ready.")
                } else if present {
                    ("warning", palette.error, "Found, but it could not start.")
                } else if failed {
                    ("warning", palette.error, "Setup failed.")
                } else if installing || pending {
                    ("clock", palette.accent, "Checking the environment…")
                } else {
                    ("info", palette.warning, "Not installed yet.")
                };
                components::notice(
                    ui,
                    icon,
                    tone,
                    self.python["python_detail"].as_str().unwrap_or(title),
                );
                components::copyable_path(ui, &path, "No interpreter path yet");
                ui.horizontal_wrapped(|ui| {
                    let ready = managed && !self.busy && self.benchmarks["status"] != "running";
                    for (icon, command, label) in [
                        ("info", "--version", "Version"),
                        ("package", "list", "Installed packages"),
                        ("copy", "freeze", "Full package list"),
                    ] {
                        if components::icon_button(ui, icon, label, ready).clicked() {
                            self.post("/api/python/pip", json!({"command": command}));
                        }
                    }
                    if components::icon_button(ui, "refresh", "Refresh", self.connected).clicked() {
                        self.get("/api/python/info");
                    }
                });
                if !managed
                    && !installing
                    && components::primary_button_enabled(
                        ui,
                        if present {
                            "Repair Python runtime"
                        } else {
                            "Set up Python runtime"
                        },
                        self.connected && !self.busy,
                        240.0,
                    )
                    .clicked()
                {
                    self.page = RUNTIME_PAGE;
                    self.start_recommended_runtime_setup();
                }
            },
        );

        ui.add_space(theme::space::MD);
        components::section(
            ui,
            "terminal",
            "Output",
            "Results of the commands above.",
            |ui| {
                if self.logs.is_empty() {
                    components::empty_state(
                        ui,
                        "denoise",
                        "Nothing yet",
                        "Run a command and its output appears here.",
                    );
                } else {
                    components::terminal(
                        ui,
                        "python-output",
                        self.logs
                            .iter()
                            .rev()
                            .take(20)
                            .rev()
                            .flat_map(|l| l.lines()),
                        360.0,
                    );
                }
            },
        );
    }
}
