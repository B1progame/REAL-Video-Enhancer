#![windows_subsystem = "windows"]

use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke, Vec2};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

const API: &str = "http://127.0.0.1:51837";
const APP_VERSION: &str = match option_env!("RVE_APP_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
const PAGES: [(&str, &str); 6] = [
    ("Home", "home"),
    ("Process", "process"),
    ("Downloads", "download"),
    ("Runtime", "runtime"),
    ("Python", "python"),
    ("Settings", "settings"),
];
const ACCENTS: [(&str, [u8; 3]); 5] = [
    ("Rust", [205, 111, 72]),
    ("Copper", [180, 101, 66]),
    ("Amber", [219, 156, 67]),
    ("Moss", [131, 159, 107]),
    ("Ice", [112, 168, 201]),
];

#[derive(Clone)]
enum ApiCommand {
    Get(String),
    Post(String, Value),
}

struct ApiEvent {
    endpoint: String,
    value: Result<Value, String>,
}

struct ApiBridge {
    send: Sender<ApiCommand>,
    receive: Receiver<ApiEvent>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .build()
        .into()
}

fn api_get(client: &ureq::Agent, endpoint: &str) -> Result<Value, String> {
    client
        .get(format!("{API}{endpoint}"))
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_json::<Value>()
        .map_err(|error| error.to_string())
}

fn api_post(client: &ureq::Agent, endpoint: &str, payload: &Value) -> Result<Value, String> {
    client
        .post(format!("{API}{endpoint}"))
        .send_json(payload)
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_json::<Value>()
        .map_err(|error| error.to_string())
}

fn start_api_worker() -> ApiBridge {
    let (command_send, command_receive) = mpsc::channel::<ApiCommand>();
    let (event_send, event_receive) = mpsc::channel::<ApiEvent>();
    thread::Builder::new()
        .name("rve-api".to_owned())
        .spawn(move || {
            let client = agent();
            let mut next_refresh = Instant::now();
            let mut connected = false;
            loop {
                match command_receive.recv_timeout(Duration::from_millis(120)) {
                    Ok(ApiCommand::Get(endpoint)) => {
                        let value = api_get(&client, &endpoint);
                        if endpoint == "/api/bootstrap" && value.is_ok() {
                            connected = true;
                        }
                        let _ = event_send.send(ApiEvent { endpoint, value });
                    }
                    Ok(ApiCommand::Post(endpoint, payload)) => {
                        let value = api_post(&client, &endpoint, &payload);
                        let _ = event_send.send(ApiEvent { endpoint, value });
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }

                if Instant::now() >= next_refresh {
                    let endpoints: &[&str] = if connected {
                        &[
                            "/api/render/status",
                            "/api/models/download_status",
                            "/api/startup/install_status",
                            "/api/animescripter/status",
                            "/api/storage/status",
                            "/api/app/update/status",
                        ]
                    } else {
                        &["/api/bootstrap"]
                    };
                    for endpoint in endpoints {
                        let value = api_get(&client, endpoint);
                        if *endpoint == "/api/bootstrap" && value.is_ok() {
                            connected = true;
                        }
                        let _ = event_send.send(ApiEvent {
                            endpoint: (*endpoint).to_owned(),
                            value,
                        });
                    }
                    next_refresh = Instant::now() + Duration::from_millis(1500);
                }
            }
        })
        .ok();
    ApiBridge {
        send: command_send,
        receive: event_receive,
    }
}

fn app_directory() -> PathBuf {
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    if let Some(dir) = executable_dir {
        if dir.join("backend").is_dir()
            || dir.join("RVE-Engine.exe").is_file()
            || dir.join("RVE-Engine").join("RVE-Engine.exe").is_file()
        {
            return dir;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .to_path_buf()
}

fn start_engine(model_storage: &str, first_setup: bool) -> Option<Child> {
    let root = app_directory();
    let bundled_name = if cfg!(windows) {
        "RVE-Engine.exe"
    } else {
        "rve-engine"
    };
    let bundled = [
        root.join(bundled_name),
        root.join("RVE-Engine").join(bundled_name),
    ]
    .into_iter()
    .find(|path| path.is_file());
    let mut command = if let Some(bundled) = bundled {
        Command::new(bundled)
    } else {
        let python = std::env::var_os("RVE_PYTHON")
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                [
                    root.join(".venv/Scripts/python.exe"),
                    root.join("venv/Scripts/python.exe"),
                    root.join(".venv/bin/python"),
                    root.join(".venv-wsl/bin/python"),
                    root.join("venv/bin/python"),
                ]
                .into_iter()
                .find(|path| path.is_file())
            })
            .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "python" } else { "python3" }));
        let mut command = Command::new(python);
        command.args(["-c", "from backend.web_api import run_server; run_server()"]);
        if cfg!(target_os = "linux") {
            let data = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".local/share/REAL-Video-Enhancer");
            command.env("RVE_APP_DATA", data);
        }
        command
    };

    command
        .current_dir(&root)
        .env("RVE_API_PORT", "51837")
        .env("RVE_MODEL_STORAGE", model_storage)
        .env("RVE_APP_VERSION", APP_VERSION)
        .env("RVE_AUTO_SETUP_MODELS", if first_setup { "0" } else { "1" })
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().ok()
}

fn apply_theme(ctx: &egui::Context, accent: Color32, appearance: &str) {
    let light = appearance == "light";
    let accent = accent_for_theme(accent, light);
    let mut visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.window_fill = if light {
        Color32::from_rgb(246, 247, 249)
    } else {
        Color32::from_rgb(24, 26, 29)
    };
    visuals.panel_fill = if light {
        Color32::from_rgb(246, 247, 249)
    } else {
        Color32::from_rgb(24, 26, 29)
    };
    visuals.extreme_bg_color = if light {
        Color32::WHITE
    } else {
        Color32::from_rgb(18, 20, 23)
    };
    visuals.faint_bg_color = if light {
        Color32::from_rgb(236, 238, 241)
    } else {
        Color32::from_rgb(31, 34, 38)
    };
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke = Stroke::new(
        1.0,
        if light {
            Color32::from_rgb(24, 26, 29)
        } else {
            Color32::WHITE
        },
    );
    visuals.widgets.active.bg_fill = accent;
    visuals.widgets.hovered.bg_fill = accent.gamma_multiply(if light { 0.18 } else { 0.35 });
    visuals.widgets.inactive.bg_fill = if light {
        Color32::from_rgb(231, 233, 237)
    } else {
        Color32::from_rgb(42, 45, 49)
    };
    visuals.widgets.noninteractive.bg_fill = visuals.window_fill;
    visuals.widgets.noninteractive.fg_stroke.color = if light {
        Color32::from_rgb(39, 41, 45)
    } else {
        Color32::from_rgb(224, 226, 229)
    };
    visuals.window_corner_radius = egui::CornerRadius::same(14);
    visuals.menu_corner_radius = egui::CornerRadius::same(10);
    ctx.set_visuals(visuals);
}

fn surface_color(light: bool) -> Color32 {
    if light {
        Color32::WHITE
    } else {
        Color32::from_rgb(31, 34, 38)
    }
}

fn border_color(light: bool) -> Color32 {
    if light {
        Color32::from_rgb(220, 222, 226)
    } else {
        Color32::from_rgb(48, 51, 55)
    }
}

fn secondary_text_color(light: bool) -> Color32 {
    if light {
        Color32::from_rgb(91, 95, 102)
    } else {
        Color32::from_rgb(161, 165, 171)
    }
}

fn accent_for_theme(accent: Color32, light: bool) -> Color32 {
    accent.gamma_multiply(if light { 0.72 } else { 1.28 })
}

fn contrast_text(background: Color32) -> Color32 {
    let [red, green, blue, _] = background.to_array();
    if (u32::from(red) * 299 + u32::from(green) * 587 + u32::from(blue) * 114) / 1000 > 145 {
        Color32::from_rgb(24, 26, 29)
    } else {
        Color32::WHITE
    }
}

fn paint_page_icon(painter: &egui::Painter, center: egui::Pos2, name: &str, color: Color32) {
    let stroke = Stroke::new(1.7, color);
    let p = |x: f32, y: f32| center + Vec2::new(x, y);
    match name {
        "home" => {
            painter.line_segment([p(-8.0, -1.0), p(0.0, -8.0)], stroke);
            painter.line_segment([p(0.0, -8.0), p(8.0, -1.0)], stroke);
            painter.line_segment([p(-6.0, -2.0), p(-6.0, 7.0)], stroke);
            painter.line_segment([p(6.0, -2.0), p(6.0, 7.0)], stroke);
            painter.line_segment([p(-6.0, 7.0), p(6.0, 7.0)], stroke);
            painter.line_segment([p(-1.5, 7.0), p(-1.5, 2.0)], stroke);
            painter.line_segment([p(1.5, 7.0), p(1.5, 2.0)], stroke);
        }
        "process" => {
            painter.add(egui::Shape::convex_polygon(
                vec![p(-5.0, -7.0), p(7.0, 0.0), p(-5.0, 7.0)],
                color,
                Stroke::NONE,
            ));
        }
        "download" => {
            painter.line_segment([p(0.0, -8.0), p(0.0, 4.0)], stroke);
            painter.line_segment([p(-5.0, -1.0), p(0.0, 4.0)], stroke);
            painter.line_segment([p(5.0, -1.0), p(0.0, 4.0)], stroke);
            painter.line_segment([p(-7.0, 7.0), p(7.0, 7.0)], stroke);
        }
        "runtime" => {
            painter.circle_stroke(p(-5.0, -5.0), 2.4, stroke);
            painter.circle_stroke(p(5.0, -5.0), 2.4, stroke);
            painter.circle_stroke(p(0.0, 5.0), 2.4, stroke);
            painter.line_segment([p(-3.0, -4.0), p(-1.0, 3.0)], stroke);
            painter.line_segment([p(3.0, -4.0), p(1.0, 3.0)], stroke);
            painter.line_segment([p(-2.5, -5.0), p(2.5, -5.0)], stroke);
        }
        "python" => {
            painter.line_segment([p(-2.0, -7.0), p(-6.0, 0.0)], stroke);
            painter.line_segment([p(-6.0, 0.0), p(-2.0, 7.0)], stroke);
            painter.line_segment([p(2.0, -7.0), p(6.0, 0.0)], stroke);
            painter.line_segment([p(6.0, 0.0), p(2.0, 7.0)], stroke);
        }
        _ => {
            painter.circle_stroke(center, 5.0, stroke);
            painter.circle_filled(center, 1.7, color);
            for angle in [0.0_f32, 0.785, 1.57, 2.355] {
                let direction = Vec2::angled(angle);
                painter.line_segment([center - direction * 7.0, center - direction * 5.5], stroke);
                painter.line_segment([center + direction * 5.5, center + direction * 7.0], stroke);
            }
        }
    }
}

fn color_from_array(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

fn settings_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
        });
    base.join("REAL-Video-Enhancer").join("native-ui.json")
}

fn saved_accent() -> Color32 {
    let Ok(contents) = std::fs::read_to_string(settings_path()) else {
        return color_from_array(ACCENTS[0].1);
    };
    let Ok(config) = serde_json::from_str::<Value>(&contents) else {
        return color_from_array(ACCENTS[0].1);
    };
    let Some(array) = config.get("accent").and_then(Value::as_array) else {
        return color_from_array(ACCENTS[0].1);
    };
    let rgb: Vec<_> = array
        .iter()
        .filter_map(Value::as_u64)
        .map(|v| v as u8)
        .collect();
    if let [red, green, blue] = rgb.as_slice() {
        Color32::from_rgb(*red, *green, *blue)
    } else {
        color_from_array(ACCENTS[0].1)
    }
}

fn saved_appearance() -> String {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .and_then(|config| {
            config
                .get("appearance")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|mode| mode == "light" || mode == "dark")
        .unwrap_or_else(|| "dark".to_owned())
}

fn saved_reduce_motion() -> bool {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .and_then(|config| config.get("reduce_motion").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn saved_model_storage() -> String {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .and_then(|config| {
            config
                .get("model_storage")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|mode| mode == "wsl" || mode == "windows")
        .unwrap_or_else(|| if cfg!(windows) { "wsl" } else { "windows" }.to_owned())
}

fn model_storage_configured() -> bool {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .and_then(|config| {
            config
                .get("model_storage")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|mode| mode == "wsl" || mode == "windows")
}

fn update_ui_setting(key: &str, value: Value) {
    let path = settings_path();
    let mut data = std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    data[key] = value;
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(serialized) = serde_json::to_string_pretty(&data) {
        let _ = std::fs::write(path, serialized);
    }
}

fn save_accent(accent: Color32) {
    let [red, green, blue, _] = accent.to_array();
    update_ui_setting("accent", json!([red, green, blue]));
}

struct NativeApp {
    api: ApiBridge,
    _engine: Option<Child>,
    page: usize,
    accent: Color32,
    appearance: String,
    reduce_motion: bool,
    connected: bool,
    bootstrap: Value,
    models: Value,
    render: Value,
    download: Value,
    install: Value,
    anime: Value,
    python: Value,
    storage_status: Value,
    update: Value,
    model_storage: String,
    first_setup: bool,
    backend: String,
    input_path: String,
    output_folder: String,
    selected_models: HashMap<String, String>,
    toggles: HashMap<String, bool>,
    interpolate_factor: f32,
    upscale_scale: i32,
    precision: String,
    tile_size: i32,
    overlap: i32,
    gpu_id: i32,
    auto_hdr: bool,
    uhd_mode: bool,
    auto_crop: bool,
    overwrite: bool,
    logs: Vec<String>,
    busy: bool,
    launch_time: Instant,
}

impl NativeApp {
    fn new(ctx: &eframe::CreationContext<'_>) -> Self {
        let accent = saved_accent();
        let appearance = saved_appearance();
        let reduce_motion = saved_reduce_motion();
        let model_storage = saved_model_storage();
        let first_setup = !model_storage_configured();
        apply_theme(&ctx.egui_ctx, accent, &appearance);
        let mut toggles = HashMap::new();
        for key in [
            "interpolate",
            "upscale",
            "decompress",
            "denoise",
            "deblur",
            "slomo",
        ] {
            toggles.insert(key.to_owned(), key == "upscale");
        }
        Self {
            api: start_api_worker(),
            _engine: start_engine(&model_storage, first_setup),
            page: if first_setup { 5 } else { 0 },
            accent,
            appearance,
            reduce_motion,
            connected: false,
            bootstrap: Value::Null,
            models: Value::Null,
            render: Value::Null,
            download: Value::Null,
            install: Value::Null,
            anime: Value::Null,
            python: Value::Null,
            storage_status: Value::Null,
            update: Value::Null,
            model_storage,
            first_setup,
            backend: "pytorch".to_owned(),
            input_path: String::new(),
            output_folder: String::new(),
            selected_models: HashMap::new(),
            toggles,
            interpolate_factor: 2.0,
            upscale_scale: 0,
            precision: "auto".to_owned(),
            tile_size: 0,
            overlap: 0,
            gpu_id: 0,
            auto_hdr: true,
            uhd_mode: false,
            auto_crop: false,
            overwrite: false,
            logs: Vec::new(),
            busy: false,
            launch_time: Instant::now(),
        }
    }

    fn get(&self, endpoint: &str) {
        let _ = self.api.send.send(ApiCommand::Get(endpoint.to_owned()));
    }

    fn post(&mut self, endpoint: &str, payload: Value) {
        self.busy = true;
        let _ = self
            .api
            .send
            .send(ApiCommand::Post(endpoint.to_owned(), payload));
    }

    fn consume_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.api.receive.try_recv() {
            self.busy = false;
            match event.value {
                Ok(value) => match event.endpoint.as_str() {
                    "/api/bootstrap" => {
                        self.bootstrap = value;
                        self.connected = true;
                        self.get(&format!("/api/models?backend={}", self.backend));
                    }
                    path if path.starts_with("/api/models?backend=") || path == "/api/models" => {
                        self.models = value
                    }
                    "/api/render/status" => self.render = value,
                    "/api/models/download_status" => self.download = value,
                    "/api/startup/install_status" => self.install = value,
                    "/api/animescripter/status" => self.anime = value,
                    "/api/python/info" => self.python = value,
                    "/api/storage/status" => self.storage_status = value,
                    "/api/app/update/status" | "/api/app/update/check" => self.update = value,
                    "/api/dialog/select_input" => {
                        self.input_path = value["path"].as_str().unwrap_or_default().to_owned();
                        if let Some(parent) = Path::new(&self.input_path).parent() {
                            self.output_folder = parent.to_string_lossy().into_owned();
                        }
                    }
                    "/api/dialog/select_output" => {
                        self.output_folder = value["path"].as_str().unwrap_or_default().to_owned();
                    }
                    _ => {
                        let detail = value["detail"]
                            .as_str()
                            .or_else(|| value["error"].as_str())
                            .or_else(|| value["status"].as_str())
                            .unwrap_or("Request completed.");
                        self.logs.push(detail.to_owned());
                    }
                },
                Err(error) => {
                    if event.endpoint == "/api/bootstrap" {
                        self.connected = false;
                    }
                    self.logs.push(error);
                }
            }
            if self.logs.len() > 50 {
                self.logs.drain(..self.logs.len() - 50);
            }
            ctx.request_repaint();
        }
    }

    fn model_choices(&self, category: &str) -> Vec<String> {
        self.models[category]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }

    fn model_picker(&mut self, ui: &mut egui::Ui, category: &str, label: &str) {
        let options = self.model_choices(category);
        let selected = self.selected_models.entry(category.to_owned()).or_default();
        let display = if selected.is_empty() {
            "Choose model".to_owned()
        } else {
            selected.clone()
        };
        egui::ComboBox::from_label(label)
            .selected_text(display)
            .width(310.0)
            .show_ui(ui, |ui| {
                for option in options {
                    ui.selectable_value(selected, option.clone(), option);
                }
            });
    }

    fn start_render(&mut self) {
        let mut selected = serde_json::Map::new();
        for key in ["upscale", "interpolate", "decompress", "denoise", "deblur"] {
            let value = self
                .selected_models
                .get(key)
                .filter(|value| !value.is_empty());
            selected.insert(
                key.to_owned(),
                value.map_or(Value::Null, |value| json!(value)),
            );
        }
        let toggles = json!({
            "upscale": self.toggles.get("upscale").copied().unwrap_or(false),
            "interpolate": self.toggles.get("interpolate").copied().unwrap_or(false),
            "decompress": self.toggles.get("decompress").copied().unwrap_or(false),
            "denoise": self.toggles.get("denoise").copied().unwrap_or(false),
            "deblur": self.toggles.get("deblur").copied().unwrap_or(false),
            "slomo": self.toggles.get("slomo").copied().unwrap_or(false),
        });
        let payload = json!({
            "engine": "rve",
            "backend": self.backend,
            "input_path": self.input_path,
            "output_folder": self.output_folder,
            "interpolate_factor": self.interpolate_factor,
            "override_upscale_scale": self.upscale_scale,
            "models": Value::Object(selected),
            "toggles": toggles,
            "settings": {
                "precision": self.precision,
                "tile_size": self.tile_size,
                "overlap": self.overlap,
                "pytorch_gpu_id": self.gpu_id,
                "ncnn_gpu_id": self.gpu_id,
                "auto_hdr_mode": self.auto_hdr,
                "uhd_mode": self.uhd_mode,
                "auto_border_cropping": self.auto_crop,
                "overwrite": self.overwrite,
                "preview_enabled": false,
                "merge_subtitles": true,
            },
            "encoding": {
                "video_encoder_preset": "libx264",
                "video_encoder_speed": "medium",
                "video_container": "mkv",
                "video_pixel_format": "yuv420p",
                "audio_encoder_preset": "copy_audio",
                "subtitle_encoder_preset": "copy_subtitle",
                "audio_bitrate": "192k",
                "crf": "18",
            }
        });
        self.post("/api/render/start", payload);
    }

    fn page_title(&self) -> &'static str {
        PAGES.get(self.page).map(|page| page.0).unwrap_or("Home")
    }

    fn header(&self, ui: &mut egui::Ui) {
        let light = !ui.visuals().dark_mode;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(self.page_title()).size(27.0).strong());
                ui.label(
                    RichText::new("REAL VIDEO ENHANCER")
                        .size(11.0)
                        .color(secondary_text_color(light)),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (color, label) = if self.connected {
                    (Color32::from_rgb(73, 157, 104), "Engine connected")
                } else {
                    (accent_for_theme(self.accent, light), "Starting engine")
                };
                ui.label(RichText::new(label).size(12.0).strong().color(color));
                ui.label(RichText::new("●").color(color));
            });
        });
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(14.0);
    }

    fn card<R>(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let light = !ui.visuals().dark_mode;
        egui::Frame::group(ui.style())
            .fill(surface_color(light))
            .stroke(Stroke::new(1.0, border_color(light)))
            .corner_radius(10)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.label(RichText::new(title).size(15.0).strong());
                ui.add_space(10.0);
                content(ui)
            })
            .inner
    }

    fn home_page(&mut self, ui: &mut egui::Ui) {
        let status_and_ready = |left: &mut egui::Ui, right: &mut egui::Ui, app: &mut Self| {
            Self::card(left, "Ready to enhance", |ui| {
                ui.label(
                    "Select a source video, choose enhancement models, then render to a folder.",
                );
                ui.add_space(12.0);
                if ui.button("Open processing workspace").clicked() {
                    app.page = 1;
                }
            });
            Self::card(right, "Engine status", |ui| {
                let system = &app.bootstrap["system"];
                let backends = system["available_backends"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Not installed".to_owned());
                ui.label(format!("Backends   {backends}"));
                ui.label(format!(
                    "Free disk   {} GB",
                    system["available_disk_gb"].as_f64().unwrap_or(0.0)
                ));
                ui.label(format!(
                    "Render   {}",
                    app.render["status"].as_str().unwrap_or("Idle")
                ));
            });
        };
        if ui.available_width() < 820.0 {
            Self::card(ui, "Ready to enhance", |ui| {
                ui.label("Choose a source video, select the models you need, and start a render.");
                ui.add_space(10.0);
                if ui.button("Open processing workspace").clicked() {
                    self.page = 1;
                }
            });
            ui.add_space(10.0);
            Self::card(ui, "Engine status", |ui| {
                ui.label(format!(
                    "Render · {}",
                    self.render["status"].as_str().unwrap_or("Idle")
                ));
                let system = &self.bootstrap["system"];
                ui.label(format!(
                    "Available backends · {}",
                    system["available_backends"]
                        .as_array()
                        .map(|items| items
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", "))
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| "Not installed".to_owned())
                ));
            });
        } else {
            ui.columns(2, |columns| {
                let (left, right) = columns.split_at_mut(1);
                status_and_ready(&mut left[0], &mut right[0], self)
            });
        }
        ui.add_space(12.0);
        Self::card(ui, "Quick actions", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Install recommended runtime").clicked() {
                    self.post("/api/startup/install", json!({"backend": "pytorch"}));
                    self.page = 3;
                }
                if ui.button("Get models").clicked() {
                    self.page = 2;
                }
                if ui.button("Settings").clicked() {
                    self.page = 5;
                }
            });
        });
        ui.add_space(12.0);
        Self::card(ui, "Current render", |ui| self.render_summary(ui));
    }

    fn source_and_output_card(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, "Source and output", |ui| {
            ui.label("Input video");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.input_path);
                if ui.button("Browse").clicked() {
                    self.get("/api/dialog/select_input");
                }
            });
            ui.add_space(8.0);
            ui.label("Output folder");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.output_folder);
                if ui.button("Browse").clicked() {
                    self.get("/api/dialog/select_output");
                }
            });
            ui.add_space(10.0);
            egui::ComboBox::from_label("Processing backend")
                .selected_text(&self.backend)
                .show_ui(ui, |ui| {
                    for backend in ["pytorch", "ncnn", "tensorrt"] {
                        if ui
                            .selectable_value(&mut self.backend, backend.to_owned(), backend)
                            .changed()
                        {
                            self.get(&format!("/api/models?backend={}", self.backend));
                        }
                    }
                });
        });
    }

    fn enhancement_models_card(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, "Enhancement models", |ui| {
            for (key, label) in [
                ("upscale", "Upscale"),
                ("interpolate", "Interpolate"),
                ("decompress", "Decompress"),
                ("denoise", "Denoise"),
                ("deblur", "Deblur"),
            ] {
                ui.horizontal(|ui| {
                    let enabled = self.toggles.entry(key.to_owned()).or_insert(false);
                    ui.checkbox(enabled, label);
                    self.model_picker(ui, key, "Model");
                });
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut self.interpolate_factor, 2.0..=8.0)
                        .text("Frame multiplier"),
                );
                ui.add(egui::Slider::new(&mut self.upscale_scale, 0..=4).text("Scale override"));
            });
            ui.checkbox(
                self.toggles.entry("slomo".to_owned()).or_insert(false),
                "Slow motion output",
            );
        });
    }

    fn process_page(&mut self, ui: &mut egui::Ui) {
        if ui.available_width() < 860.0 {
            self.source_and_output_card(ui);
            ui.add_space(10.0);
            self.enhancement_models_card(ui);
        } else {
            ui.columns(2, |columns| {
                self.source_and_output_card(&mut columns[0]);
                self.enhancement_models_card(&mut columns[1]);
            });
        }
        ui.add_space(12.0);
        Self::card(ui, "Render", |ui| {
            ui.horizontal(|ui| {
                let active = matches!(self.render["status"].as_str(), Some("running" | "paused"));
                if ui
                    .add_enabled(
                        !active && !self.busy,
                        egui::Button::new(RichText::new("Start render").strong()),
                    )
                    .clicked()
                {
                    self.start_render();
                }
                if active {
                    let paused = self.render["paused"].as_bool().unwrap_or(false);
                    if ui.button(if paused { "Resume" } else { "Pause" }).clicked() {
                        self.post("/api/render/pause", json!({"paused": !paused}));
                    }
                    if ui.button("Stop").clicked() {
                        self.post("/api/render/stop", json!({}));
                    }
                }
                self.render_summary(ui);
            });
            if let Some(lines) = self.render["log_lines"].as_array() {
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for line in lines.iter().rev().take(8).filter_map(Value::as_str) {
                            ui.label(RichText::new(line).monospace().size(11.0));
                        }
                    });
            }
        });
    }

    fn render_summary(&self, ui: &mut egui::Ui) {
        let status = self.render["status"].as_str().unwrap_or("idle");
        let frame = self.render["current_frame"].as_u64().unwrap_or(0);
        let fps = self.render["gen_fps"].as_f64().unwrap_or(0.0);
        let eta = self.render["eta"].as_str().unwrap_or("--");
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Status: {status}"));
            ui.label(format!("Frame: {frame}"));
            ui.label(format!("Speed: {fps:.1} FPS"));
            ui.label(format!("ETA: {eta}"));
        });
    }

    fn downloads_page(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, "Model library", |ui| {
            ui.label("Download weights only when you need them, or prepare the full library in one pass.");
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Interpolation models").clicked() {
                    self.post("/api/models/download", json!({"backend": self.backend, "categories": ["interpolate"]}));
                }
                if ui.button("Upscale and restoration").clicked() {
                    self.post("/api/models/download", json!({"backend": self.backend, "categories": ["upscale", "decompress", "denoise", "deblur"]}));
                }
                if ui.button("All models for this backend").clicked() {
                    self.post("/api/models/download", json!({"backend": self.backend, "categories": ["interpolate", "upscale", "decompress", "denoise", "deblur", "scene_change"]}));
                }
            });
            ui.add_space(12.0);
            ui.label(format!(
                "{} · {}%",
                self.download["label"].as_str().unwrap_or("Idle"),
                self.download["progress"].as_u64().unwrap_or(0)
            ));
            ui.add(
                egui::ProgressBar::new(
                    self.download["progress"].as_f64().unwrap_or(0.0) as f32 / 100.0,
                )
                .show_percentage(),
            );
            ui.label(
                self.download["detail"]
                    .as_str()
                    .unwrap_or("Models download into the engine's model folder."),
            );
        });
        ui.add_space(12.0);
        Self::card(ui, "Ubuntu on WSL", |ui| {
            ui.label("Choose WSL in Settings to start Ubuntu automatically and install or verify the full model catalog.");
            ui.label("The Windows app and Python engine stay on Windows. WSL stores model weights under your Linux home.");
        });
    }

    fn runtime_page(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, "Runtime setup", |ui| {
            ui.label(
                self.install["detail"]
                    .as_str()
                    .unwrap_or("Check this installation or install the recommended backend."),
            );
            ui.add(
                egui::ProgressBar::new(
                    self.install["progress"].as_f64().unwrap_or(0.0) as f32 / 100.0,
                )
                .text(self.install["stage"].as_str().unwrap_or("Waiting")),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button("Install recommended").clicked() {
                    self.post("/api/startup/install", json!({"backend": "pytorch"}));
                }
                if ui.button("Install NCNN").clicked() {
                    self.post(
                        "/api/actions/manage_backend",
                        json!({"action": "install", "backend": "ncnn"}),
                    );
                }
                if ui.button("Install PyTorch").clicked() {
                    self.post(
                        "/api/actions/manage_backend",
                        json!({"action": "install", "backend": "pytorch"}),
                    );
                }
                if ui.button("Install TensorRT").clicked() {
                    self.post(
                        "/api/actions/manage_backend",
                        json!({"action": "install", "backend": "tensorrt"}),
                    );
                }
            });
        });
        ui.add_space(12.0);
        Self::card(ui, "AnimeScripter · unfinished", |ui| {
            ui.label("The AnimeScripter integration code is preserved, but the native app and runtime setup are not finished for it yet. We can complete that later.");
        });
        ui.add_space(12.0);
        Self::card(ui, "Detected system", |ui| {
            let system = &self.bootstrap["system"];
            for key in [
                "os",
                "gpu_name",
                "cuda_version",
                "available_backends",
                "backend_probe_output",
            ] {
                let value = &system[key];
                if !value.is_null() {
                    ui.label(format!("{}: {}", key.replace('_', " "), value));
                }
            }
            if ui.button("Refresh detection").clicked() {
                self.post("/api/actions/refresh_backends", json!({}));
                self.get("/api/bootstrap");
            }
        });
    }

    fn python_page(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, "Managed Python environment", |ui| {
            ui.label(format!(
                "Interpreter: {}",
                self.python["python_path"].as_str().unwrap_or("Checking...")
            ));
            ui.label(
                self.python["python_detail"]
                    .as_str()
                    .unwrap_or("The runtime page installs the inference environment."),
            );
            ui.horizontal(|ui| {
                for command in ["--version", "list", "freeze"] {
                    if ui.button(format!("pip {command}")).clicked() {
                        self.post("/api/python/pip", json!({"command": command}));
                    }
                }
                if ui.button("Refresh Python info").clicked() {
                    self.get("/api/python/info");
                }
            });
        });
        ui.add_space(12.0);
        Self::card(ui, "Activity", |ui| {
            egui::ScrollArea::vertical()
                .max_height(330.0)
                .show(ui, |ui| {
                    for line in self.logs.iter().rev().take(20).rev() {
                        ui.label(RichText::new(line).monospace().size(11.0));
                    }
                });
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, "Appearance", |ui| {
            let light = !ui.visuals().dark_mode;
            ui.label("Interface theme");
            ui.horizontal(|ui| {
                for (label, value) in [("Light", "light"), ("Dark", "dark")] {
                    if ui
                        .selectable_label(self.appearance == value, label)
                        .clicked()
                    {
                        self.appearance = value.to_owned();
                        update_ui_setting("appearance", json!(self.appearance));
                        apply_theme(ui.ctx(), self.accent, &self.appearance);
                    }
                }
            });
            let mut reduce_motion = self.reduce_motion;
            if ui
                .checkbox(&mut reduce_motion, "Reduce animation")
                .changed()
            {
                self.reduce_motion = reduce_motion;
                update_ui_setting("reduce_motion", json!(reduce_motion));
            }
            ui.add_space(8.0);
            ui.label("Accent color");
            ui.horizontal_wrapped(|ui| {
                for (label, rgb) in ACCENTS {
                    let color = color_from_array(rgb);
                    let response = ui.add_sized(
                        Vec2::new(108.0, 44.0),
                        egui::Button::new(RichText::new(label).color(contrast_text(color)))
                            .fill(color),
                    );
                    if response.clicked() {
                        self.accent = color;
                        save_accent(color);
                        apply_theme(ui.ctx(), color, &self.appearance);
                    }
                }
                if ui.color_edit_button_srgba(&mut self.accent).changed() {
                    save_accent(self.accent);
                    apply_theme(ui.ctx(), self.accent, &self.appearance);
                }
            });
            ui.label(
                RichText::new(
                    "Your color is saved locally and updates the Rust interface immediately.",
                )
                .size(11.0)
                .color(secondary_text_color(light)),
            );
        });
        ui.add_space(12.0);
        Self::card(ui, "Model storage", |ui| {
            ui.label("Choose where model weights are installed and read from.");
            egui::ComboBox::from_label("Storage location")
                .selected_text(if self.model_storage == "wsl" {
                    "WSL Ubuntu · Ubuntai"
                } else {
                    "This PC · Windows"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.model_storage,
                        "wsl".to_owned(),
                        "WSL Ubuntu · Ubuntai",
                    );
                    ui.selectable_value(
                        &mut self.model_storage,
                        "windows".to_owned(),
                        "This PC · Windows",
                    );
                });
            if ui
                .button("Apply and install or verify all models")
                .clicked()
            {
                update_ui_setting("model_storage", json!(self.model_storage));
                self.first_setup = false;
                self.post(
                    "/api/storage/configure",
                    json!({"storage": self.model_storage}),
                );
            }
            if self.first_setup {
                ui.label("First launch: choose where to keep the models, then start setup. WSL starts automatically if selected.");
            }
            let status = self.storage_status["status"].as_str().unwrap_or("waiting");
            let stage = self.storage_status["stage"]
                .as_str()
                .unwrap_or("Waiting for engine status");
            let detail = self.storage_status["detail"].as_str().unwrap_or("");
            ui.add_space(6.0);
            ui.label(format!("{stage} · {status}"));
            if !detail.is_empty() {
                ui.label(
                    RichText::new(detail)
                        .size(11.0)
                        .color(secondary_text_color(!ui.visuals().dark_mode)),
                );
            }
            if let Some(progress) = self.storage_status["progress"].as_f64() {
                ui.add(
                    egui::ProgressBar::new((progress as f32 / 100.0).clamp(0.0, 1.0).into())
                        .show_percentage(),
                );
            }
            if let Some(error) = self.storage_status["error"]
                .as_str()
                .filter(|value| !value.is_empty())
            {
                ui.colored_label(Color32::from_rgb(240, 125, 115), error);
            }
            ui.label(
                RichText::new(
                    self.storage_status["model_dir"]
                        .as_str()
                        .unwrap_or("Storage path will be shown after setup"),
                )
                .size(11.0)
                .monospace(),
            );
        });
        ui.add_space(12.0);
        Self::card(ui, "Updates", |ui| {
            ui.label(format!("Installed version · {APP_VERSION}"));
            ui.label(
                self.update["detail"]
                    .as_str()
                    .unwrap_or("Check GitHub for a newer Windows release."),
            );
            if let Some(url) = self.update["release_url"].as_str() {
                ui.hyperlink_to("View release notes on GitHub", url);
            }
            if let Some(progress) = self.update["progress"].as_f64() {
                if self.update["status"] == "downloading" || self.update["status"] == "downloaded" {
                    ui.add(
                        egui::ProgressBar::new((progress as f32 / 100.0).clamp(0.0, 1.0))
                            .show_percentage(),
                    );
                }
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Check GitHub for updates").clicked() {
                    self.get("/api/app/update/check");
                }
                if self.update["update_available"].as_bool() == Some(true)
                    && self.update["status"] != "downloading"
                    && self.update["status"] != "downloaded"
                    && ui.button("Download update").clicked()
                {
                    self.post("/api/app/update/download", json!({}));
                }
                if self.update["status"] == "downloaded"
                    && ui.button("Install and restart").clicked()
                {
                    if let Some(path) = self.update["installer_path"].as_str() {
                        if Command::new(path).spawn().is_ok() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
            });
        });
        ui.add_space(12.0);
        Self::card(ui, "Render defaults", |ui| {
            egui::ComboBox::from_label("Precision")
                .selected_text(&self.precision)
                .show_ui(ui, |ui| {
                    for option in ["auto", "float16", "float32"] {
                        ui.selectable_value(&mut self.precision, option.to_owned(), option);
                    }
                });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.tile_size)
                        .range(0..=4096)
                        .prefix("Tile size "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.overlap)
                        .range(0..=256)
                        .prefix("Overlap "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.gpu_id)
                        .range(0..=64)
                        .prefix("GPU id "),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.auto_hdr, "Automatic HDR");
                ui.checkbox(&mut self.uhd_mode, "UHD mode");
                ui.checkbox(&mut self.auto_crop, "Detect black borders");
                ui.checkbox(&mut self.overwrite, "Replace existing output");
            });
        });
    }

    fn draw_startup_animation(&self, ctx: &egui::Context) {
        if self.reduce_motion {
            return;
        }
        let elapsed = self.launch_time.elapsed().as_secs_f32();
        if elapsed >= 1.9 {
            return;
        }
        let fade_in = (elapsed / 0.32).clamp(0.0, 1.0);
        let fade_out = ((1.9 - elapsed) / 0.45).clamp(0.0, 1.0);
        let opacity = fade_in.min(fade_out);
        let light = self.appearance == "light";
        let screen = ctx.content_rect();
        let center = screen.center();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("rve-startup"),
        ));
        painter.rect_filled(
            screen,
            0.0,
            Color32::from_black_alpha((92.0 * opacity) as u8),
        );
        let card = egui::Rect::from_center_size(center, Vec2::new(330.0, 174.0));
        painter.rect_filled(
            card.translate(Vec2::new(0.0, 7.0)),
            18.0,
            Color32::from_black_alpha((115.0 * opacity) as u8),
        );
        painter.rect_filled(
            card,
            18.0,
            if light {
                Color32::from_rgba_unmultiplied(250, 250, 251, (255.0 * opacity) as u8)
            } else {
                Color32::from_rgba_unmultiplied(29, 32, 36, (255.0 * opacity) as u8)
            },
        );
        painter.rect_stroke(
            card,
            18.0,
            Stroke::new(1.0, border_color(light).gamma_multiply(opacity)),
            egui::StrokeKind::Inside,
        );
        let orbit = center + Vec2::new(0.0, -35.0);
        let phase = elapsed * 4.3;
        for index in 0..3 {
            let angle = phase + index as f32 * std::f32::consts::TAU / 3.0;
            let point = orbit + Vec2::angled(angle) * 15.0;
            let pulse = 0.65 + 0.35 * (phase + index as f32).sin().abs();
            painter.circle_filled(
                point,
                4.0,
                accent_for_theme(self.accent, light).gamma_multiply(opacity * pulse),
            );
        }
        painter.text(
            center + Vec2::new(0.0, 13.0),
            egui::Align2::CENTER_CENTER,
            "REAL VIDEO ENHANCER",
            egui::FontId::proportional(16.0),
            if light {
                Color32::from_black_alpha((228.0 * opacity) as u8)
            } else {
                Color32::from_white_alpha((245.0 * opacity) as u8)
            },
        );
        painter.text(
            center + Vec2::new(0.0, 40.0),
            egui::Align2::CENTER_CENTER,
            "Starting your local engine…",
            egui::FontId::proportional(12.0),
            if light {
                Color32::from_gray((85.0 * opacity) as u8)
            } else {
                Color32::from_gray((166.0 * opacity) as u8)
            },
        );
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl eframe::App for NativeApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        self.consume_events(&ctx);
        let shortcut = ctx.input(|input| {
            if input.modifiers.ctrl {
                [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                    egui::Key::Num6,
                ]
                .iter()
                .position(|key| input.key_pressed(*key))
            } else {
                None
            }
        });
        if let Some(page) = shortcut {
            self.page = page;
        }
        let light = self.appearance == "light";
        let theme_accent = accent_for_theme(self.accent, light);
        let compact_sidebar = root_ui.available_width() < 1120.0;
        let sidebar_width = if compact_sidebar { 72.0 } else { 216.0 };
        egui::Panel::left("navigation")
            .resizable(false)
            .default_size(sidebar_width)
            .min_size(sidebar_width)
            .max_size(sidebar_width)
            .frame(
                egui::Frame::new()
                    .fill(if light {
                        Color32::from_rgb(246, 247, 249)
                    } else {
                        Color32::from_rgb(24, 26, 29)
                    })
                    .stroke(Stroke::new(1.0, border_color(light)))
                    .inner_margin(if compact_sidebar { 9 } else { 16 }),
            )
            .show(root_ui, |ui| {
                ui.add_space(4.0);
                if compact_sidebar {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        ui.label(RichText::new("RVE").size(21.0).strong().color(self.accent));
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("RVE").size(22.0).strong().color(self.accent));
                        ui.vertical(|ui| {
                            ui.label(RichText::new("REAL VIDEO").size(11.0).strong());
                            ui.label(
                                RichText::new("ENHANCER")
                                    .size(9.0)
                                    .color(secondary_text_color(light)),
                            );
                        });
                    });
                }
                ui.add_space(if compact_sidebar { 20.0 } else { 32.0 });
                if !compact_sidebar {
                    ui.label(
                        RichText::new("WORKSPACE")
                            .size(10.0)
                            .strong()
                            .color(secondary_text_color(light)),
                    );
                    ui.add_space(8.0);
                }
                for (index, (label, icon)) in PAGES.iter().enumerate() {
                    let selected = self.page == index;
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(
                            if compact_sidebar {
                                48.0
                            } else {
                                sidebar_width - 32.0
                            },
                            44.0,
                        ),
                        egui::Sense::click(),
                    );
                    let fill = if selected {
                        theme_accent.gamma_multiply(if light { 0.13 } else { 0.18 })
                    } else if response.hovered() {
                        ui.visuals().widgets.hovered.bg_fill
                    } else {
                        Color32::TRANSPARENT
                    };
                    let painter = ui.painter();
                    painter.rect_filled(rect, 8.0, fill);
                    let icon_color = if selected {
                        theme_accent
                    } else {
                        secondary_text_color(light)
                    };
                    paint_page_icon(
                        painter,
                        if compact_sidebar {
                            rect.center()
                        } else {
                            egui::pos2(rect.left() + 21.0, rect.center().y)
                        },
                        icon,
                        icon_color,
                    );
                    if !compact_sidebar {
                        painter.text(
                            egui::pos2(rect.left() + 43.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            *label,
                            egui::FontId::proportional(13.0),
                            if selected {
                                ui.visuals().text_color()
                            } else {
                                secondary_text_color(light)
                            },
                        );
                    }
                    if response.has_focus() {
                        painter.rect_stroke(
                            rect,
                            8.0,
                            Stroke::new(1.0, theme_accent),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let response = response.on_hover_text(format!("{label} · Ctrl+{}", index + 1));
                    if response.clicked() {
                        self.page = index;
                    }
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            self.page == index,
                            *label,
                        )
                    });
                    ui.add_space(3.0);
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    let state = if self.connected {
                        "● Engine ready"
                    } else {
                        "● Starting"
                    };
                    let response = ui.label(
                        RichText::new(if compact_sidebar { "●" } else { state })
                            .size(11.0)
                            .color(if self.connected {
                                Color32::from_rgb(73, 157, 104)
                            } else {
                                self.accent
                            }),
                    );
                    let _ = response.on_hover_text(if compact_sidebar {
                        state
                    } else {
                        "Engine status"
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(if light {
                        Color32::from_rgb(242, 243, 245)
                    } else {
                        Color32::from_rgb(19, 21, 24)
                    })
                    .inner_margin(egui::Margin::symmetric(
                        if compact_sidebar { 16 } else { 30 },
                        22,
                    )),
            )
            .show(root_ui, |ui| {
                self.header(ui);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        0 => self.home_page(ui),
                        1 => self.process_page(ui),
                        2 => self.downloads_page(ui),
                        3 => self.runtime_page(ui),
                        4 => self.python_page(ui),
                        5 => self.settings_page(ui),
                        _ => self.home_page(ui),
                    });
            });

        self.draw_startup_animation(&ctx);

        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("REAL Video Enhancer")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([840.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "REAL Video Enhancer",
        options,
        Box::new(|ctx| Ok(Box::new(NativeApp::new(ctx)))),
    )
}
