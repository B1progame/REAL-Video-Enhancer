//! The Studio's output location and per-render options.

use eframe::egui::{self, Vec2};
use serde_json::json;

use crate::{
    NativeApp, RUNTIME_PAGE, backend_is_installed,
    ui::{components, theme},
};

const BACKENDS: [&str; 3] = ["pytorch", "ncnn", "tensorrt"];

impl NativeApp {
    pub(crate) fn output_configuration_card(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        components::section(
            ui,
            "output",
            "Output",
            "The enhanced file is written here.",
            |ui| {
                if self.output_to_mpv {
                    components::notice(
                        ui,
                        "play",
                        palette.accent,
                        "Sending enhanced video to MPV instead of saving it.",
                    );
                    return;
                }
                ui.add_sized(
                    Vec2::new(ui.available_width(), 36.0),
                    egui::TextEdit::singleline(&mut self.output_folder)
                        .margin(Vec2::new(12.0, 9.0))
                        .hint_text("Choose an output folder"),
                );
                if components::icon_button(ui, "folder", "Browse…", true).clicked() {
                    self.get("/api/dialog/select_output");
                }
            },
        );
    }

    pub(crate) fn advanced_processing_card(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let installed = self.bootstrap["system"]["available_backends"].clone();
        let mut changed = false;
        let mut preview = self.saved_settings["advanced"]["preview_enabled"]
            .as_bool()
            .unwrap_or(true);
        components::section(
            ui,
            "sliders",
            "Fine tuning",
            "Backend, factors and tiling for this render.",
            |ui| {
                let mut index = BACKENDS
                    .iter()
                    .position(|name| *name == self.backend)
                    .unwrap_or(0);
                if components::segmented(
                    ui,
                    "studio-backend",
                    &["PyTorch", "NCNN", "TensorRT"],
                    &mut index,
                ) {
                    self.backend = BACKENDS[index].to_owned();
                    self.get(&format!("/api/models?backend={}", self.backend));
                    changed = true;
                }
                if !backend_is_installed(&installed, &self.backend) {
                    components::notice(
                        ui,
                        "warning",
                        palette.warning,
                        "This backend is not installed yet.",
                    );
                    if components::icon_button(ui, "wrench", "Open Runtime", true).clicked() {
                        self.page = RUNTIME_PAGE;
                    }
                }
                components::row(
                    ui,
                    "interpolate",
                    "Frame multiplier",
                    "For interpolation.",
                    |ui| {
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut self.interpolate_factor, 2.0..=8.0)
                                    .suffix("×"),
                            )
                            .changed();
                    },
                );
                components::row(
                    ui,
                    "upscale",
                    "Scale override",
                    "0 keeps the model's.",
                    |ui| {
                        changed |= ui
                            .add(egui::Slider::new(&mut self.upscale_scale, 0..=4).suffix("×"))
                            .changed();
                    },
                );
                components::row(ui, "target", "Precision", "", |ui| {
                    egui::ComboBox::from_id_salt("studio-precision")
                        .selected_text(&self.precision)
                        .show_ui(ui, |ui| {
                            for option in ["auto", "float16", "float32"] {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.precision,
                                        option.to_owned(),
                                        option,
                                    )
                                    .changed();
                            }
                        });
                });
                components::row(ui, "grid", "Tile size / overlap", "0 is automatic.", |ui| {
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.overlap).range(0..=256))
                        .changed();
                    changed |= ui
                        .add(egui::DragValue::new(&mut self.tile_size).range(0..=4096))
                        .changed();
                });
                ui.add_space(theme::space::XS);
                ui.horizontal_wrapped(|ui| {
                    for (icon, label, value) in [
                        ("sun", "HDR", &mut self.auto_hdr),
                        ("monitor", "UHD", &mut self.uhd_mode),
                        ("crop", "Crop borders", &mut self.auto_crop),
                        ("save", "Replace output", &mut self.overwrite),
                        ("eye", "Live preview", &mut preview),
                    ] {
                        if components::chip(ui, Some(icon), label, *value).clicked() {
                            *value = !*value;
                            changed = true;
                        }
                    }
                });
            },
        );
        if changed {
            if !self.saved_settings["advanced"].is_object() {
                self.saved_settings["advanced"] = json!({});
            }
            self.saved_settings["advanced"]["preview_enabled"] = json!(preview);
            self.save_render_defaults();
        }
    }
}
