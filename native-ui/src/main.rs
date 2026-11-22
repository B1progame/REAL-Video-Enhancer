#![windows_subsystem = "windows"]

use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke, Vec2};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU16, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};
mod ui;
use ui::theme::{accent_for_theme, border_color, contrast_text, secondary_text_color};
use ui::{components, icons, theme};

static API_PORT: AtomicU16 = AtomicU16::new(51837);
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
        .get(format!(
            "http://127.0.0.1:{}{endpoint}",
            API_PORT.load(Ordering::Relaxed)
        ))
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_json::<Value>()
        .map_err(|error| error.to_string())
}

fn api_post(client: &ureq::Agent, endpoint: &str, payload: &Value) -> Result<Value, String> {
    client
        .post(format!(
            "http://127.0.0.1:{}{endpoint}",
            API_PORT.load(Ordering::Relaxed)
        ))
        .send_json(payload)
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_json::<Value>()
        .map_err(|error| error.to_string())
}

fn start_api_worker(ctx: egui::Context) -> ApiBridge {
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
                        ctx.request_repaint();
                    }
                    Ok(ApiCommand::Post(endpoint, payload)) => {
                        let value = api_post(&client, &endpoint, &payload);
                        let _ = event_send.send(ApiEvent { endpoint, value });
                        ctx.request_repaint();
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
                        } else if *endpoint != "/api/bootstrap"
                            && let Err(error) = &value
                        {
                            let _ = event_send.send(ApiEvent {
                                endpoint: "/api/connection".to_owned(),
                                value: Err(error.clone()),
                            });
                            connected = false;
                            ctx.request_repaint();
                            break;
                        }
                        let _ = event_send.send(ApiEvent {
                            endpoint: (*endpoint).to_owned(),
                            value,
                        });
                        ctx.request_repaint();
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
    if let Some(dir) = executable_dir
        && (dir.join("backend").is_dir()
            || dir.join("RVE-Engine.exe").is_file()
            || dir.join("RVE-Engine").join("RVE-Engine.exe").is_file())
    {
        return dir;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .to_path_buf()
}

fn start_engine(model_storage: &str, first_setup: bool) -> Result<Option<Child>, String> {
    let probe_client = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(450)))
        .build()
        .into();
    let mut candidate_ports = Vec::new();
    if let Some(port) = saved_api_port() {
        candidate_ports.push(port);
    }
    if !candidate_ports.contains(&51837) {
        candidate_ports.push(51837);
    }
    for port in candidate_ports {
        API_PORT.store(port, Ordering::Relaxed);
        if api_get(&probe_client, "/api/bootstrap")
            .is_ok_and(|value| value["system"].is_object() && value["models"].is_object())
        {
            // A later app launch can share the existing local service. The
            // app only shuts down a child process that it started itself.
            return Ok(None);
        }
    }

    // Port 51837 may be excluded by Windows or already owned by another
    // service. Ask the OS for a free loopback port instead of making startup
    // depend on a fixed port being available.
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Could not reserve a local engine port: {error}"))?;
    let api_port = listener
        .local_addr()
        .map_err(|error| format!("Could not read the selected local engine port: {error}"))?
        .port();
    drop(listener);
    API_PORT.store(api_port, Ordering::Relaxed);
    remember_api_port(api_port);

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
        .env("RVE_API_PORT", api_port.to_string())
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
    let program = command.get_program().to_string_lossy().into_owned();
    command
        .spawn()
        .map(Some)
        .map_err(|error| format!("Could not start local engine `{program}`: {error}"))
}

fn apply_theme(ctx: &egui::Context, accent: Color32, appearance: &str) {
    theme::apply(ctx, accent, appearance);
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

fn api_port_path() -> PathBuf {
    settings_path().with_file_name("engine-api-port.txt")
}

fn saved_api_port() -> Option<u16> {
    std::fs::read_to_string(api_port_path())
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|port| *port != 0)
}

fn remember_api_port(port: u16) {
    let path = api_port_path();
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let _ = std::fs::write(path, port.to_string());
}

fn saved_accent() -> Color32 {
    let Ok(contents) = std::fs::read_to_string(settings_path()) else {
        return color_from_array(theme::ACCENTS[0].1);
    };
    let Ok(config) = serde_json::from_str::<Value>(&contents) else {
        return color_from_array(theme::ACCENTS[0].1);
    };
    let Some(array) = config.get("accent").and_then(Value::as_array) else {
        return color_from_array(theme::ACCENTS[0].1);
    };
    let rgb: Vec<_> = array
        .iter()
        .filter_map(Value::as_u64)
        .map(|v| v as u8)
        .collect();
    if let [red, green, blue] = rgb.as_slice() {
        Color32::from_rgb(*red, *green, *blue)
    } else {
        color_from_array(theme::ACCENTS[0].1)
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
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return;
    }
    if let Ok(serialized) = serde_json::to_string_pretty(&data) {
        let _ = std::fs::write(path, serialized);
    }
}

fn save_accent(accent: Color32) {
    let [red, green, blue, _] = accent.to_array();
    update_ui_setting("accent", json!([red, green, blue]));
}

fn default_encoding() -> Value {
    json!({
        "video_encoder_preset": "libx264",
        "video_encoder_speed": "medium",
        "video_container": "mkv",
        "video_pixel_format": "yuv420p",
        "audio_encoder_preset": "copy_audio",
        "subtitle_encoder_preset": "copy_subtitle",
        "audio_bitrate": "192k",
        "crf": "18",
        "video_quality": "High"
    })
}

struct NativeApp {
    api: ApiBridge,
    _engine: Option<Child>,
    page: usize,
    accent: Color32,
    appearance: String,
    reduce_motion: bool,
    connected: bool,
    engine_error: Option<String>,
    input_info: Value,
    bootstrap: Value,
    models: Value,
    render: Value,
    download: Value,
    install: Value,
    anime: Value,
    python: Value,
    storage_status: Value,
    update: Value,
    saved_settings: Value,
    encoding: Value,
    model_storage: String,
    first_setup: bool,
    backend: String,
    input_path: String,
    output_folder: String,
    output_to_mpv: bool,
    selected_models: HashMap<String, String>,
    toggles: HashMap<String, bool>,
    interpolate_factor: f32,
    upscale_scale: i32,
    precision: String,
    tile_size: i32,
    overlap: i32,
    pytorch_gpu_id: i32,
    ncnn_gpu_id: i32,
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
        let (engine, engine_error) = match start_engine(&model_storage, first_setup) {
            Ok(engine) => (engine, None),
            Err(error) => (None, Some(error)),
        };
        egui_extras::install_image_loaders(&ctx.egui_ctx);
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
            api: start_api_worker(ctx.egui_ctx.clone()),
            _engine: engine,
            page: if first_setup { 5 } else { 0 },
            accent,
            appearance,
            reduce_motion,
            connected: false,
            engine_error,
            input_info: Value::Null,
            bootstrap: Value::Null,
            models: Value::Null,
            render: Value::Null,
            download: Value::Null,
            install: Value::Null,
            anime: Value::Null,
            python: Value::Null,
            storage_status: Value::Null,
            update: Value::Null,
            saved_settings: json!({}),
            encoding: default_encoding(),
            model_storage,
            first_setup,
            backend: "pytorch".to_owned(),
            input_path: String::new(),
            output_folder: String::new(),
            output_to_mpv: false,
            selected_models: HashMap::new(),
            toggles,
            interpolate_factor: 2.0,
            upscale_scale: 0,
            precision: "auto".to_owned(),
            tile_size: 0,
            overlap: 0,
            pytorch_gpu_id: 0,
            ncnn_gpu_id: 0,
            auto_hdr: true,
            uhd_mode: false,
            auto_crop: false,
            overwrite: false,
            logs: Vec::new(),
            busy: false,
            launch_time: Instant::now(),
        }
    }

    fn restore_backend_settings(&mut self, settings: &Value) {
        self.saved_settings = if settings.is_object() {
            settings.clone()
        } else {
            json!({})
        };
        let advanced = &self.saved_settings["advanced"];
        let configured_backend = self.saved_settings["backend"].as_str().unwrap_or("pytorch");
        self.backend = if configured_backend == "auto" {
            "pytorch".to_owned()
        } else {
            configured_backend.to_owned()
        };
        for (key, default) in [
            ("interpolate", false),
            ("upscale", true),
            ("decompress", false),
            ("denoise", false),
            ("deblur", false),
            ("slomo", false),
        ] {
            self.toggles.insert(
                key.to_owned(),
                self.saved_settings["toggles"][key]
                    .as_bool()
                    .unwrap_or(default),
            );
        }
        if let Some(output_folder) = advanced["output_folder_location"].as_str() {
            self.output_folder = output_folder.to_owned();
        }
        self.output_to_mpv = advanced["output_to_mpv"].as_bool().unwrap_or(false);
        self.precision = advanced["precision"].as_str().unwrap_or("auto").to_owned();
        self.tile_size = advanced["tile_size"].as_i64().unwrap_or(0) as i32;
        self.overlap = advanced["overlap"].as_i64().unwrap_or(0) as i32;
        self.pytorch_gpu_id = advanced["pytorch_gpu_id"].as_i64().unwrap_or(0) as i32;
        self.ncnn_gpu_id = advanced["ncnn_gpu_id"].as_i64().unwrap_or(0) as i32;
        self.interpolate_factor = advanced["interpolate_factor"].as_f64().unwrap_or(2.0) as f32;
        self.upscale_scale = advanced["override_upscale_scale"].as_i64().unwrap_or(0) as i32;
        for key in ["upscale", "interpolate", "decompress", "denoise", "deblur"] {
            if let Some(model) = self.saved_settings["models"][key].as_str() {
                self.selected_models
                    .insert(key.to_owned(), model.to_owned());
            }
        }
        self.auto_hdr = advanced["auto_hdr_mode"].as_bool().unwrap_or(true);
        self.uhd_mode = advanced["uhd_mode"].as_bool().unwrap_or(false);
        self.auto_crop = advanced["auto_border_cropping"].as_bool().unwrap_or(false);
        self.overwrite = advanced["overwrite"].as_bool().unwrap_or(false);

        let mut encoding = default_encoding();
        if let (Some(defaults), Some(saved)) = (
            encoding.as_object_mut(),
            self.saved_settings["encoding"].as_object(),
        ) {
            defaults.extend(saved.clone());
        }
        self.encoding = encoding;
    }

    fn save_render_defaults(&mut self) {
        if !self.saved_settings.is_object() {
            self.saved_settings = json!({});
        }
        if !self.saved_settings["advanced"].is_object() {
            self.saved_settings["advanced"] = json!({});
        }
        self.saved_settings["backend"] = json!(self.backend);
        self.saved_settings["toggles"] = json!(self.toggles);
        self.saved_settings["encoding"] = self.encoding.clone();
        let advanced = &mut self.saved_settings["advanced"];
        advanced["output_folder_location"] = json!(self.output_folder);
        advanced["output_to_mpv"] = json!(self.output_to_mpv);
        advanced["precision"] = json!(self.precision);
        advanced["tile_size"] = json!(self.tile_size);
        advanced["overlap"] = json!(self.overlap);
        advanced["pytorch_gpu_id"] = json!(self.pytorch_gpu_id);
        advanced["ncnn_gpu_id"] = json!(self.ncnn_gpu_id);
        advanced["interpolate_factor"] = json!(self.interpolate_factor);
        advanced["override_upscale_scale"] = json!(self.upscale_scale);
        advanced["auto_hdr_mode"] = json!(self.auto_hdr);
        advanced["uhd_mode"] = json!(self.uhd_mode);
        advanced["auto_border_cropping"] = json!(self.auto_crop);
        advanced["overwrite"] = json!(self.overwrite);
        self.saved_settings["models"] = json!(self.selected_models);
        self.post(
            "/api/settings",
            json!({"settings": self.saved_settings.clone()}),
        );
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
                        let settings = self.bootstrap["settings"].clone();
                        self.restore_backend_settings(&settings);
                        self.connected = true;
                        self.engine_error = None;
                        self.get(&format!("/api/models?backend={}", self.backend));
                        self.get("/api/python/info");
                    }
                    path if path.starts_with("/api/models?backend=") || path == "/api/models" => {
                        self.models = value
                    }
                    "/api/render/status" => self.render = value,
                    "/api/models/download_status" => self.download = value,
                    "/api/startup/install_status" => self.install = value,
                    "/api/animescripter/status" => self.anime = value,
                    "/api/python/info" => self.python = value,
                    "/api/video/inspect" => self.input_info = value,
                    "/api/python/pip" => {
                        let command = value["command"].as_str().unwrap_or("pip");
                        let output = value["output"]
                            .as_str()
                            .unwrap_or("No output was returned.");
                        self.logs.push(format!("$ {command}\n{output}"));
                    }
                    "/api/storage/status" => self.storage_status = value,
                    "/api/app/update/status"
                    | "/api/app/update/check"
                    | "/api/app/update/download" => self.update = value,
                    "/api/dialog/select_input" => {
                        self.input_path = value["path"].as_str().unwrap_or_default().to_owned();
                        self.input_info = value["video_info"].clone();
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
                    if event.endpoint == "/api/bootstrap" || event.endpoint == "/api/connection" {
                        self.connected = false;
                        self.engine_error = Some(error.clone());
                    }
                    if matches!(
                        event.endpoint.as_str(),
                        "/api/app/update/check"
                            | "/api/app/update/status"
                            | "/api/app/update/download"
                    ) {
                        self.update = json!({
                            "status": "error",
                            "current_version": APP_VERSION,
                            "detail": format!("Update request failed: {error}")
                        });
                    }
                    if event.endpoint == "/api/video/inspect" {
                        self.input_info = json!({"error": error});
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

    fn model_picker(&mut self, ui: &mut egui::Ui, category: &str) -> bool {
        let options = self.model_choices(category);
        let selected = self.selected_models.entry(category.to_owned()).or_default();
        let display = if selected.is_empty() {
            "Choose model".to_owned()
        } else {
            selected.clone()
        };
        let picker_width = ui.available_width().clamp(120.0, 310.0);
        let mut changed = false;
        egui::ComboBox::from_id_salt(("model-picker", category))
            .selected_text(display)
            .width(picker_width)
            .show_ui(ui, |ui| {
                for option in options {
                    changed |= ui
                        .selectable_value(selected, option.clone(), option)
                        .changed();
                }
            });
        changed
    }

    fn source_drop_target(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let available = ui.available_width().max(180.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(available, 100.0), egui::Sense::click());
        let files_hovered = ui.input(|input| !input.raw.hovered_files.is_empty());
        let fill = if response.hovered() || files_hovered {
            palette.accent.gamma_multiply(0.12)
        } else {
            palette.elevated
        };
        ui.painter().rect_filled(rect, 12.0, fill);
        ui.painter().rect_stroke(
            rect,
            12.0,
            Stroke::new(
                if response.hovered() || files_hovered {
                    1.5
                } else {
                    1.0
                },
                if response.hovered() || files_hovered {
                    palette.accent
                } else {
                    palette.border
                },
            ),
            egui::StrokeKind::Inside,
        );
        let title = if self.input_path.is_empty() {
            "Drop a video here or choose a file"
        } else {
            "Replace the source video"
        };
        let detail = if self.input_path.is_empty() {
            "MP4 · MKV · MOV · AVI"
        } else {
            Path::new(&self.input_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Selected video")
        };
        let upload_center = rect.center() - Vec2::new(0.0, 21.0);
        ui.painter()
            .circle_stroke(upload_center, 11.0, Stroke::new(1.6, palette.accent));
        ui.painter().line_segment(
            [
                upload_center - Vec2::new(0.0, 5.0),
                upload_center + Vec2::new(0.0, 4.0),
            ],
            Stroke::new(1.6, palette.accent),
        );
        ui.painter().line_segment(
            [
                upload_center + Vec2::new(0.0, 4.0),
                upload_center + Vec2::new(4.0, 0.0),
            ],
            Stroke::new(1.6, palette.accent),
        );
        ui.painter().line_segment(
            [
                upload_center + Vec2::new(0.0, 4.0),
                upload_center - Vec2::new(4.0, 0.0),
            ],
            Stroke::new(1.6, palette.accent),
        );
        ui.painter().line_segment(
            [
                upload_center + Vec2::new(-5.0, 6.0),
                upload_center + Vec2::new(5.0, 6.0),
            ],
            Stroke::new(1.6, palette.accent),
        );
        ui.painter().text(
            rect.center() + Vec2::new(0.0, 5.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(14.0),
            palette.text,
        );
        ui.painter().text(
            rect.center() + Vec2::new(0.0, 27.0),
            egui::Align2::CENTER_CENTER,
            detail,
            egui::FontId::proportional(11.0),
            palette.secondary,
        );
        if response.clicked() {
            self.get("/api/dialog/select_input");
        }
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    }

    fn start_render(&mut self) {
        self.save_render_defaults();
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
        let mut render_settings = self.saved_settings["advanced"].clone();
        if !render_settings.is_object() {
            render_settings = json!({});
        }
        render_settings["precision"] = json!(self.precision);
        render_settings["tile_size"] = json!(self.tile_size);
        render_settings["overlap"] = json!(self.overlap);
        render_settings["pytorch_gpu_id"] = json!(self.pytorch_gpu_id);
        render_settings["ncnn_gpu_id"] = json!(self.ncnn_gpu_id);
        render_settings["auto_hdr_mode"] = json!(self.auto_hdr);
        render_settings["uhd_mode"] = json!(self.uhd_mode);
        render_settings["auto_border_cropping"] = json!(self.auto_crop);
        render_settings["overwrite"] = json!(self.overwrite);
        render_settings["output_to_mpv"] = json!(self.output_to_mpv);
        render_settings["preview_enabled"] = json!(
            self.saved_settings["advanced"]["preview_enabled"]
                .as_bool()
                .unwrap_or(false)
        );
        render_settings["merge_subtitles"] = json!(
            self.saved_settings["advanced"]["merge_subtitles"]
                .as_bool()
                .unwrap_or(true)
        );
        render_settings["interpolate_factor"] = json!(self.interpolate_factor);
        render_settings["override_upscale_scale"] = json!(self.upscale_scale);
        let payload = json!({
            "engine": "rve",
            "backend": self.backend,
            "input_path": self.input_path,
            "output_folder": self.output_folder,
            "interpolate_factor": self.interpolate_factor,
            "override_upscale_scale": self.upscale_scale,
            "models": Value::Object(selected),
            "toggles": toggles,
            "settings": render_settings,
            "encoding": self.encoding.clone()
        });
        self.post("/api/render/start", payload);
    }

    fn page_title(&self) -> &'static str {
        PAGES.get(self.page).map(|page| page.0).unwrap_or("Home")
    }

    fn page_description(&self) -> &'static str {
        match self.page {
            0 => "Your workspace for cleaner, sharper video.",
            1 => "Choose a source, tune enhancements, and render.",
            2 => "Find and install the models you need.",
            3 => "Set up the inference backends for your hardware.",
            4 => "Inspect the Python engine and its environment.",
            5 => "Personalize appearance, storage, and app updates.",
            _ => "Your workspace for cleaner, sharper video.",
        }
    }

    fn header(&self, ui: &mut egui::Ui) {
        let light = !ui.visuals().dark_mode;
        let palette = theme::palette(light, self.accent);
        ui.horizontal(|ui| {
            let icon_frame = egui::Frame::new()
                .fill(palette.elevated)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(12)
                .inner_margin(egui::Margin::same(10));
            icon_frame.show(ui, |ui| {
                ui.add(icons::styled(
                    PAGES.get(self.page).map(|page| page.1).unwrap_or("home"),
                    palette.accent,
                    24.0,
                ));
            });
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(self.page_title())
                        .size(25.0)
                        .strong()
                        .color(palette.text),
                );
                ui.label(
                    RichText::new(self.page_description())
                        .size(13.0)
                        .color(palette.secondary),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (status, color) = if self.connected {
                    ("Engine connected", palette.success)
                } else if self.engine_error.is_some() {
                    ("Engine unavailable", palette.error)
                } else {
                    ("Starting engine", palette.warning)
                };
                egui::Frame::new()
                    .fill(color.gamma_multiply(if light { 0.10 } else { 0.13 }))
                    .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| components::status_dot(ui, color, status));
            });
        });
        ui.add_space(theme::space::LG);
    }

    fn card<R>(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
        components::card(ui, title, content)
    }

    fn brand_mark(ui: &mut egui::Ui, size: f32) {
        ui.add(
            egui::Image::from_bytes(
                "bytes://rve-brand-mark",
                include_bytes!("../../docs/assets/rve-logo.png").as_slice(),
            )
            .fit_to_exact_size(Vec2::splat(size))
            .corner_radius(9),
        );
    }

    fn home_page(&mut self, ui: &mut egui::Ui) {
        let light = !ui.visuals().dark_mode;
        let palette = theme::palette(light, self.accent);
        let system = &self.bootstrap["system"];
        let backends = system["available_backends"]
            .as_array()
            .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        let selected_backend_ready = backends.iter().any(|backend| *backend == self.backend);
        let model_count = self.models.as_object().map_or(0, |categories| {
            categories
                .values()
                .filter_map(Value::as_array)
                .map(Vec::len)
                .sum::<usize>()
        });
        let disk_space = system["available_disk_gb"].as_f64();

        egui::Frame::new()
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.accent.gamma_multiply(0.32)))
            .corner_radius(theme::radius::CARD)
            .inner_margin(egui::Margin::same(24))
            .show(ui, |ui| {
                ui.columns(2, |columns| {
                    let left = &mut columns[0];
                    left.label(
                        RichText::new("VIDEO RESTORATION WORKSPACE")
                            .size(10.0)
                            .strong()
                            .color(palette.accent),
                    );
                    left.add_space(theme::space::SM);
                    left.label(
                        RichText::new("Bring your footage
back into focus.")
                            .size(30.0)
                            .strong()
                            .color(palette.text),
                    );
                    left.add_space(theme::space::SM);
                    left.label(
                        RichText::new("Upscale, interpolate, and restore video with the models and engine you choose.")
                            .size(14.0)
                            .color(palette.secondary),
                    );
                    left.add_space(theme::space::LG);
                    if components::primary_button(left, "Start a new enhancement").clicked() {
                        self.page = 1;
                    }
                    left.add_space(theme::space::SM);
                    left.label(
                        RichText::new("Native Windows app  ·  Local Python engine")
                            .size(11.0)
                            .color(palette.muted),
                    );

                    let right = &mut columns[1];
                    right.vertical_centered(|ui| {
                        ui.add_space(theme::space::MD);
                        ui.add(icons::styled("process", palette.accent, 38.0));
                        ui.add_space(theme::space::MD);
                        ui.label(
                            RichText::new("Enhancement pipeline")
                                .size(15.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.add_space(theme::space::SM);
                        ui.label(
                            RichText::new("Upscale  ·  Interpolate  ·  Restore")
                                .size(12.0)
                                .color(palette.secondary),
                        );
                        ui.add_space(theme::space::MD);
                        ui.label(
                            RichText::new(format!("{} model options in the selected backend catalog", model_count))
                                .size(11.0)
                                .color(palette.muted),
                        );
                    });
                });
            });

        ui.add_space(theme::space::LG);
        ui.columns(3, |columns| {
            Self::card(&mut columns[0], "Engine", |ui| {
                components::status_dot(
                    ui,
                    if self.connected {
                        palette.success
                    } else if self.engine_error.is_some() {
                        palette.error
                    } else {
                        palette.warning
                    },
                    if self.connected {
                        "Connected"
                    } else if self.engine_error.is_some() {
                        "Unavailable"
                    } else {
                        "Starting"
                    },
                );
                ui.add_space(theme::space::XS);
                ui.label(
                    RichText::new(
                        self.engine_error
                            .as_deref()
                            .unwrap_or("The engine runs locally on this PC."),
                    )
                    .size(12.0)
                    .color(if self.engine_error.is_some() {
                        palette.error
                    } else {
                        palette.secondary
                    }),
                );
            });
            Self::card(&mut columns[1], "Inference backend", |ui| {
                components::status_dot(
                    ui,
                    if selected_backend_ready {
                        palette.success
                    } else {
                        palette.warning
                    },
                    if selected_backend_ready {
                        "Ready"
                    } else {
                        "Setup needed"
                    },
                );
                ui.add_space(theme::space::XS);
                ui.label(
                    RichText::new(format!(
                        "{} · {} available",
                        self.backend.to_uppercase(),
                        backends.len()
                    ))
                    .size(12.0)
                    .color(palette.secondary),
                );
            });
            Self::card(&mut columns[2], "Model storage", |ui| {
                let (label, color) = if self.model_storage == "wsl" {
                    ("WSL · Ubuntu", palette.accent)
                } else {
                    ("Windows", palette.secondary)
                };
                components::status_dot(ui, color, label);
                ui.add_space(theme::space::XS);
                let models = if model_count == 1 {
                    "1 catalog option"
                } else {
                    &format!("{model_count} catalog options")
                };
                ui.label(RichText::new(models).size(12.0).color(palette.secondary));
            });
        });

        ui.add_space(theme::space::LG);
        Self::card(ui, "Quick actions", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Install runtime").clicked() {
                    self.post("/api/startup/install", json!({"backend": "pytorch"}));
                    self.page = 3;
                }
                if ui.button("Manage models").clicked() {
                    self.page = 2;
                }
                if ui.button("Settings").clicked() {
                    self.page = 5;
                }
                if let Some(gb) = disk_space {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{gb:.1} GB free"))
                                .size(12.0)
                                .color(palette.muted),
                        );
                    });
                }
            });
        });

        ui.add_space(theme::space::LG);
        Self::card(ui, "Current render", |ui| self.render_summary(ui));
    }

    fn source_and_output_card(&mut self, ui: &mut egui::Ui) {
        Self::card(ui, "Source and output", |ui| {
            ui.label("Source video");
            self.source_drop_target(ui);
            ui.add_space(theme::space::SM);
            ui.horizontal(|ui| {
                let path_width = (ui.available_width() - 82.0).max(100.0);
                let path_response = ui.add_sized(
                    Vec2::new(path_width, 38.0),
                    egui::TextEdit::singleline(&mut self.input_path)
                        .hint_text("Choose a video file…"),
                );
                if path_response.changed() {
                    self.input_info = Value::Null;
                }
                if !self.input_path.trim().is_empty()
                    && ui
                        .button("Inspect")
                        .on_hover_text("Read video metadata")
                        .clicked()
                {
                    self.post(
                        "/api/video/inspect",
                        json!({"path": self.input_path.trim()}),
                    );
                }
            });
            if let Some(error) = self.input_info["error"].as_str() {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).warning,
                    error,
                );
            } else if let (Some(width), Some(height)) = (
                self.input_info["width"].as_u64(),
                self.input_info["height"].as_u64(),
            ) {
                let fps = self.input_info["fps"].as_f64().unwrap_or(0.0);
                let duration = self.input_info["duration_seconds"].as_f64().unwrap_or(0.0);
                let codec = self.input_info["codec"].as_str().unwrap_or("unknown codec");
                ui.label(
                    RichText::new(format!(
                        "{width} × {height}  ·  {fps:.2} fps  ·  {duration:.1}s  ·  {codec}"
                    ))
                    .size(12.0)
                    .color(secondary_text_color(!ui.visuals().dark_mode)),
                );
            }
            ui.add_space(8.0);
            if self.output_to_mpv {
                ui.label("Output is sent directly to MPV.");
            } else {
                ui.label("Save enhanced video to");
                ui.horizontal(|ui| {
                    let path_width = (ui.available_width() - 96.0).max(100.0);
                    ui.add_sized(
                        Vec2::new(path_width, 38.0),
                        egui::TextEdit::singleline(&mut self.output_folder)
                            .hint_text("Choose an output folder…"),
                    );
                    if ui.button("Browse…").clicked() {
                        self.get("/api/dialog/select_output");
                    }
                });
            }
            ui.add_space(10.0);
            egui::ComboBox::from_label("Processing backend")
                .selected_text(&self.backend)
                .show_ui(ui, |ui| {
                    for backend in ["pytorch", "ncnn", "tensorrt"] {
                        if ui
                            .selectable_value(&mut self.backend, backend.to_owned(), backend)
                            .changed()
                        {
                            self.save_render_defaults();
                            self.get(&format!("/api/models?backend={}", self.backend));
                        }
                    }
                });
        });
    }

    fn enhancement_models_card(&mut self, ui: &mut egui::Ui) {
        let mut selection_changed = false;
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
                    selection_changed |= ui.checkbox(enabled, label).changed();
                    selection_changed |= self.model_picker(ui, key);
                });
            }
            let interpolate_enabled = self.toggles.get("interpolate").copied().unwrap_or(false);
            let upscale_enabled = self.toggles.get("upscale").copied().unwrap_or(false);
            if interpolate_enabled {
                ui.add(
                    egui::Slider::new(&mut self.interpolate_factor, 2.0..=8.0)
                        .text("Frame multiplier"),
                );
            }
            if upscale_enabled {
                ui.add(egui::Slider::new(&mut self.upscale_scale, 0..=4).text("Scale override"));
            }
            ui.checkbox(
                self.toggles.entry("slomo".to_owned()).or_insert(false),
                "Slow motion output",
            );
        });
        if selection_changed {
            self.save_render_defaults();
        }
    }

    fn process_page(&mut self, ui: &mut egui::Ui) {
        let ready_backends = self.bootstrap["system"]["available_backends"]
            .as_array()
            .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        let backend_ready = ready_backends
            .iter()
            .any(|backend| *backend == self.backend);
        let render_preparing = self.render["status"] == "preparing";
        let active = matches!(
            self.render["status"].as_str(),
            Some("preparing" | "running" | "paused")
        );
        let mut blockers: Vec<String> = Vec::new();
        if !self.connected {
            blockers.push(self.engine_error.as_ref().map_or_else(
                || "The local engine is still starting.".to_owned(),
                |error| format!("The local engine is unavailable: {error}"),
            ));
        }
        if self.input_path.trim().is_empty() {
            blockers.push("Choose an input video.".to_owned());
        } else if !Path::new(self.input_path.trim()).is_file() {
            blockers.push("The selected input path is not an existing file.".to_owned());
        }
        if !self.output_to_mpv && self.output_folder.trim().is_empty() {
            blockers.push("Choose an output folder.".to_owned());
        }
        if !backend_ready {
            blockers
                .push("Install or select an available processing backend in Runtime.".to_owned());
        }
        let enabled = ["upscale", "interpolate", "decompress", "denoise", "deblur"]
            .into_iter()
            .filter(|key| self.toggles.get(*key).copied().unwrap_or(false))
            .collect::<Vec<_>>();
        if enabled.is_empty() {
            blockers.push("Enable at least one enhancement.".to_owned());
        }
        let missing_model = enabled.iter().any(|key| {
            self.selected_models
                .get(*key)
                .is_none_or(|model| model.is_empty())
        });
        if missing_model {
            blockers.push("Choose a model for each enabled enhancement.".to_owned());
        }
        let can_start = blockers.is_empty() && !active && !self.busy;

        if ui.available_width() < 860.0 {
            self.source_and_output_card(ui);
            ui.add_space(theme::space::MD);
            self.enhancement_models_card(ui);
        } else {
            ui.columns(2, |columns| {
                self.source_and_output_card(&mut columns[0]);
                self.enhancement_models_card(&mut columns[1]);
            });
        }
        ui.add_space(theme::space::MD);
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        Self::card(ui, "Render", |ui| {
            ui.horizontal_wrapped(|ui| {
                let response = components::primary_button_enabled(
                    ui,
                    if active {
                        if render_preparing {
                            "Preparing render…"
                        } else {
                            "Render in progress"
                        }
                    } else {
                        "Start render"
                    },
                    can_start,
                    220.0,
                );
                if response.clicked() {
                    self.start_render();
                }
                if active && !render_preparing {
                    let paused = self.render["paused"].as_bool().unwrap_or(false);
                    if ui.button(if paused { "Resume" } else { "Pause" }).clicked() {
                        self.post("/api/render/pause", json!({"paused": !paused}));
                    }
                    if ui.button("Stop render").clicked() {
                        self.post("/api/render/stop", json!({}));
                    }
                }
                if self.busy && !active {
                    ui.label(RichText::new("Preparing request…").color(palette.secondary));
                }
            });
            if !can_start {
                let message = if render_preparing {
                    "Preparing the renderer and selected models."
                } else if active {
                    "A render is already running."
                } else if self.busy {
                    "Waiting for the engine to finish its current action."
                } else {
                    blockers
                        .first()
                        .map(String::as_str)
                        .unwrap_or("Complete the setup before rendering.")
                };
                ui.label(RichText::new(message).color(palette.warning));
                if blockers.len() > 1 && !active && !self.busy {
                    ui.label(
                        RichText::new(blockers[1..].join("  "))
                            .size(12.0)
                            .color(palette.secondary),
                    );
                }
            }
            ui.add_space(theme::space::MD);
            self.render_summary(ui);
            if let Some(error) = self.render["error"]
                .as_str()
                .filter(|value| !value.is_empty())
            {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).error,
                    error,
                );
            }
            if let Some(lines) = self.render["log_lines"].as_array() {
                egui::CollapsingHeader::new("Render activity")
                    .id_salt("render-activity")
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(140.0)
                            .show(ui, |ui| {
                                for line in lines.iter().rev().take(8).filter_map(Value::as_str) {
                                    ui.label(RichText::new(line).monospace().size(11.0));
                                }
                            });
                    });
            }
        });
    }

    fn render_summary(&self, ui: &mut egui::Ui) {
        let status = self.render["status"].as_str().unwrap_or("idle");
        let frame = self.render["current_frame"].as_u64().unwrap_or(0);
        let fps = self.render["gen_fps"].as_f64();
        let eta = self.render["eta"].as_str().unwrap_or("--");
        let light = !ui.visuals().dark_mode;
        let palette = theme::palette(light, self.accent);
        let status_color = match status {
            "preparing" | "running" => palette.accent,
            "paused" => palette.warning,
            "completed" => palette.success,
            "error" => palette.error,
            _ => palette.secondary,
        };
        ui.horizontal_wrapped(|ui| {
            components::status_dot(ui, status_color, &status.to_uppercase());
            if frame > 0 {
                ui.label(format!("Frame {frame}"));
            }
            if let Some(fps) = fps {
                ui.label(format!("{fps:.1} FPS"));
            }
            if eta != "--" && !eta.is_empty() {
                ui.label(format!("ETA {eta}"));
            }
            if let Some(output) = self.render["output_file"].as_str() {
                ui.label(
                    RichText::new(output)
                        .size(11.0)
                        .monospace()
                        .color(palette.muted),
                );
            }
        });
    }

    fn downloads_page(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let active = self.download["active"].as_bool().unwrap_or(false);
        let busy = self.busy;
        let catalog = self.models.as_object().map_or_else(Vec::new, |categories| {
            categories
                .iter()
                .filter(|(name, value)| value.is_array() && name.as_str() != "model_status")
                .map(|(name, names)| {
                    let names = names
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    let installed =
                        self.models["model_status"][name]
                            .as_object()
                            .map_or(0, |status| {
                                status
                                    .values()
                                    .filter(|value| value.as_bool() == Some(true))
                                    .count()
                            });
                    (name.replace('_', " "), name.clone(), names, installed)
                })
                .collect::<Vec<_>>()
        });

        Self::card(ui, "Download models", |ui| {
            ui.label(RichText::new("Download only the model weights you need. They are stored in the location selected in Settings.").color(palette.secondary));
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("Backend · {}", self.backend.to_uppercase())).strong(),
                );
                ui.label(
                    RichText::new(if self.model_storage == "wsl" {
                        "Storage · WSL Ubuntu"
                    } else {
                        "Storage · Windows"
                    })
                    .color(palette.secondary),
                );
            });
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                for (label, categories) in [
                    ("Interpolation", vec!["interpolate"]),
                    (
                        "Restoration",
                        vec!["upscale", "decompress", "denoise", "deblur"],
                    ),
                    (
                        "All for this backend",
                        vec![
                            "interpolate",
                            "upscale",
                            "decompress",
                            "denoise",
                            "deblur",
                            "scene_change",
                        ],
                    ),
                ] {
                    let enabled = !active && !busy;
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        self.post(
                            "/api/models/download",
                            json!({"backend": self.backend, "categories": categories}),
                        );
                    }
                }
            });

            if active
                || self.download["status"]
                    .as_str()
                    .is_some_and(|status| status != "idle")
            {
                ui.add_space(theme::space::MD);
                ui.horizontal(|ui| {
                    components::status_dot(
                        ui,
                        if active {
                            palette.accent
                        } else if self.download["status"] == "error" {
                            palette.error
                        } else {
                            palette.success
                        },
                        self.download["stage"]
                            .as_str()
                            .unwrap_or_else(|| self.download["status"].as_str().unwrap_or("Ready")),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{:.0}%",
                                self.download["progress"].as_f64().unwrap_or(0.0)
                            ))
                            .strong(),
                        );
                    });
                });
                ui.add(egui::ProgressBar::new(
                    (self.download["progress"].as_f64().unwrap_or(0.0) as f32 / 100.0)
                        .clamp(0.0, 1.0),
                ));
                if let Some(label) = self.download["label"]
                    .as_str()
                    .filter(|label| !label.is_empty())
                {
                    ui.label(RichText::new(label).strong());
                }
                if let Some(detail) = self.download["detail"]
                    .as_str()
                    .filter(|detail| !detail.is_empty())
                {
                    ui.label(RichText::new(detail).size(12.0).color(palette.secondary));
                }
                if let Some(error) = self.download["error"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                {
                    ui.colored_label(palette.error, error);
                }
            }
        });

        ui.add_space(theme::space::MD);
        Self::card(ui, "Model catalog", |ui| {
            ui.label(
                RichText::new("Installed counts come from the engine’s local model scan.")
                    .size(12.0)
                    .color(palette.secondary),
            );
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    if catalog.is_empty() {
                        ui.label("The catalog is loading from the selected backend.");
                    }
                    for (display, key, names, installed) in &catalog {
                        egui::CollapsingHeader::new(format!(
                            "{display}  ·  {installed}/{} installed",
                            names.len()
                        ))
                        .id_salt(("model-category", key))
                        .show(ui, |ui| {
                            for name in names {
                                let ready =
                                    self.models["model_status"][key][name].as_bool() == Some(true);
                                ui.horizontal(|ui| {
                                    components::status_dot(
                                        ui,
                                        if ready {
                                            palette.success
                                        } else {
                                            palette.muted
                                        },
                                        if ready { "Installed" } else { "Available" },
                                    );
                                    ui.label(name);
                                });
                            }
                        });
                    }
                });
        });

        ui.add_space(theme::space::MD);
        Self::card(ui, "Model location", |ui| {
            let path = self.storage_status["model_dir"]
                .as_str()
                .unwrap_or("Choose a model location in Settings, then run setup.");
            ui.label(RichText::new(path).monospace().size(12.0));
            ui.add_space(theme::space::XS);
            ui.label(RichText::new(if self.model_storage == "wsl" {
                "Weights are installed in Ubuntu under WSL. The Windows app and Python engine remain on Windows."
            } else {
                "Weights are installed on this Windows PC alongside the app’s local data."
            }).size(12.0).color(palette.secondary));
        });
    }

    fn runtime_page(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let status = self.install["status"]
            .as_str()
            .unwrap_or("not_started")
            .to_owned();
        let progress = self.install["progress"]
            .as_f64()
            .unwrap_or(0.0)
            .clamp(0.0, 100.0) as f32;
        let installing = matches!(status.as_str(), "running" | "installing" | "downloading")
            || (progress > 0.0 && progress < 100.0);
        let system = self.bootstrap["system"].clone();
        let available = system["available_backends"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "No inference backend detected".to_owned());

        Self::card(ui, "Inference runtime", |ui| {
            ui.horizontal(|ui| {
                components::status_dot(
                    ui,
                    if installing {
                        palette.accent
                    } else if available.starts_with("No ") {
                        palette.warning
                    } else {
                        palette.success
                    },
                    if installing {
                        "Installing"
                    } else if available.starts_with("No ") {
                        "Setup required"
                    } else {
                        "Ready"
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(status.replace('_', " "))
                            .size(12.0)
                            .color(palette.secondary),
                    );
                });
            });
            ui.label(
                self.install["detail"]
                    .as_str()
                    .unwrap_or("Install a rendering backend to enable video enhancement."),
            );
            if progress > 0.0 || installing {
                ui.add(
                    egui::ProgressBar::new(progress / 100.0).text(
                        self.install["stage"]
                            .as_str()
                            .unwrap_or("Preparing runtime"),
                    ),
                );
            }
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                let enabled = !installing && !self.busy;
                if ui
                    .add_enabled(enabled, egui::Button::new("Install recommended runtime"))
                    .clicked()
                {
                    self.post("/api/startup/install", json!({"backend": "pytorch"}));
                }
                for backend in ["ncnn", "pytorch", "tensorrt"] {
                    let label = format!("Install {}", backend.to_uppercase());
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        self.post(
                            "/api/actions/manage_backend",
                            json!({"action": "install", "backend": backend}),
                        );
                    }
                }
            });
        });

        ui.add_space(theme::space::MD);
        Self::card(ui, "Hardware and compatibility", |ui| {
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    for (key, label) in [("os", "Operating system"), ("cuda_version", "CUDA")] {
                        ui.label(RichText::new(label).size(11.0).color(palette.muted));
                        ui.label(system[key].as_str().unwrap_or_else(|| {
                            if system[key].is_null() {
                                "Not detected"
                            } else {
                                "Unknown"
                            }
                        }));
                    }
                });
                columns[1].vertical(|ui| {
                    for (key, label) in [
                        ("gpu_name", "Graphics device"),
                        ("available_backends", "Available backends"),
                    ] {
                        ui.label(RichText::new(label).size(11.0).color(palette.muted));
                        ui.label(system[key].as_str().unwrap_or_else(|| {
                            if system[key].is_null() {
                                "Not detected"
                            } else {
                                "Unknown"
                            }
                        }));
                    }
                });
            });
            if ui.button("Refresh system detection").clicked() {
                self.post("/api/actions/refresh_backends", json!({}));
                self.get("/api/bootstrap");
            }
        });

        ui.add_space(theme::space::MD);
        Self::card(ui, "AnimeScripter · unfinished", |ui| {
            ui.label("AnimeScripter integration code is preserved in the project, but setup and the native workflow are not finished. It is not ready for normal processing yet; we can complete it later.");
        });
    }

    fn python_page(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let available = self.python["python_available"].as_bool().unwrap_or(false);
        let python_path = self.python["python_path"].as_str().unwrap_or("").to_owned();
        Self::card(ui, "Managed Python", |ui| {
            ui.horizontal(|ui| {
                components::status_dot(
                    ui,
                    if available {
                        palette.success
                    } else {
                        palette.warning
                    },
                    if available {
                        "Interpreter ready"
                    } else {
                        "Not installed"
                    },
                );
                if ui.button("Refresh environment").clicked() {
                    self.get("/api/python/info");
                }
            });
            ui.label(
                self.python["python_detail"]
                    .as_str()
                    .unwrap_or("The Runtime page prepares the local Python engine."),
            );
            ui.add_space(theme::space::SM);
            ui.label(
                RichText::new("Interpreter path")
                    .size(11.0)
                    .color(palette.muted),
            );
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if python_path.is_empty() {
                        "Waiting for engine…"
                    } else {
                        &python_path
                    })
                    .monospace()
                    .size(12.0),
                );
                if !python_path.is_empty() && ui.button("Copy path").clicked() {
                    ui.ctx().copy_text(python_path.clone());
                }
            });
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                for (command, label) in [
                    ("--version", "Python version"),
                    ("list", "Installed packages"),
                    ("freeze", "Full package list"),
                ] {
                    if ui
                        .add_enabled(available && !self.busy, egui::Button::new(label))
                        .clicked()
                    {
                        self.post("/api/python/pip", json!({"command": command}));
                    }
                }
            });
        });
        ui.add_space(theme::space::MD);
        Self::card(ui, "Recent activity", |ui| {
            if self.logs.is_empty() {
                ui.label(
                    RichText::new("Python and setup command output will appear here.")
                        .color(palette.secondary),
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for line in self.logs.iter().rev().take(20).rev() {
                            ui.label(RichText::new(line).monospace().size(12.0));
                        }
                    });
            }
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
                for (label, rgb) in theme::ACCENTS {
                    let color = color_from_array(rgb);
                    let response = ui.add_sized(
                        Vec2::new(118.0, 42.0),
                        egui::Button::new(
                            RichText::new(label).strong().color(contrast_text(color)),
                        )
                        .fill(color)
                        .stroke(Stroke::new(
                            if self.accent == color { 2.0 } else { 1.0 },
                            if self.accent == color {
                                ui.visuals().text_color()
                            } else {
                                color
                            },
                        )),
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
                    egui::ProgressBar::new((progress as f32 / 100.0).clamp(0.0, 1.0))
                        .show_percentage(),
                );
            }
            if let Some(error) = self.storage_status["error"]
                .as_str()
                .filter(|value| !value.is_empty())
            {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).error,
                    error,
                );
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
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Installed").color(secondary_text_color(!ui.visuals().dark_mode)),
                );
                ui.label(RichText::new(format!("v{APP_VERSION}")).strong());
                if let Some(version) = self.update["latest_version"].as_str() {
                    ui.separator();
                    ui.label(
                        RichText::new("Latest")
                            .color(secondary_text_color(!ui.visuals().dark_mode)),
                    );
                    ui.label(RichText::new(format!("v{version}")).strong());
                }
            });
            ui.label(
                self.update["detail"]
                    .as_str()
                    .unwrap_or("Check B1progame on GitHub for the latest Windows release."),
            );
            ui.hyperlink_to(
                "Release channel · B1progame/REAL-Video-Enhancer",
                "https://github.com/B1progame/REAL-Video-Enhancer/releases",
            );
            if self.update["status"] == "error" {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).error,
                    "Update check needs attention",
                );
            }
            if self.update["status"] == "downloaded" {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).success,
                    "Installer downloaded and ready",
                );
            }
            if let Some(url) = self.update["release_url"].as_str() {
                ui.hyperlink_to("View release notes on GitHub", url);
            }
            if let Some(progress) = self.update["progress"].as_f64()
                && (self.update["status"] == "downloading" || self.update["status"] == "downloaded")
            {
                ui.add(
                    egui::ProgressBar::new((progress as f32 / 100.0).clamp(0.0, 1.0))
                        .show_percentage(),
                );
            }
            ui.horizontal_wrapped(|ui| {
                let checking = self.update["status"] == "checking";
                let check_label = if checking {
                    "Checking for updates…"
                } else {
                    "Check for updates"
                };
                if ui
                    .add_enabled(!checking, egui::Button::new(check_label))
                    .clicked()
                {
                    self.update = json!({
                        "status": "checking",
                        "current_version": APP_VERSION,
                        "detail": "Checking the B1progame GitHub releases…"
                    });
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
                    && ui.button("Install downloaded update").clicked()
                    && let Some(path) = self.update["installer_path"].as_str().map(str::to_owned)
                {
                    match Command::new(path).spawn() {
                        Ok(_) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(error) => {
                            self.update["status"] = json!("error");
                            self.update["detail"] =
                                json!(format!("Could not start the update installer: {error}"));
                        }
                    }
                }
            });
        });
        ui.add_space(12.0);
        let mut render_defaults_changed = false;
        Self::card(ui, "Render defaults", |ui| {
            egui::ComboBox::from_label("Precision")
                .selected_text(&self.precision)
                .show_ui(ui, |ui| {
                    for option in ["auto", "float16", "float32"] {
                        render_defaults_changed |= ui
                            .selectable_value(&mut self.precision, option.to_owned(), option)
                            .changed();
                    }
                });
            ui.horizontal(|ui| {
                render_defaults_changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.tile_size)
                            .range(0..=4096)
                            .prefix("Tile size "),
                    )
                    .changed();
                render_defaults_changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.overlap)
                            .range(0..=256)
                            .prefix("Overlap "),
                    )
                    .changed();
                render_defaults_changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.pytorch_gpu_id)
                            .range(0..=64)
                            .prefix("PyTorch GPU "),
                    )
                    .changed();
                render_defaults_changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.ncnn_gpu_id)
                            .range(0..=64)
                            .prefix("NCNN GPU "),
                    )
                    .changed();
            });
            ui.horizontal_wrapped(|ui| {
                render_defaults_changed |= ui
                    .checkbox(&mut self.output_to_mpv, "Send output to MPV")
                    .changed();
                render_defaults_changed |=
                    ui.checkbox(&mut self.auto_hdr, "Automatic HDR").changed();
                render_defaults_changed |= ui.checkbox(&mut self.uhd_mode, "UHD mode").changed();
                render_defaults_changed |= ui
                    .checkbox(&mut self.auto_crop, "Detect black borders")
                    .changed();
                render_defaults_changed |= ui
                    .checkbox(&mut self.overwrite, "Replace existing output")
                    .changed();
            });
        });
        if render_defaults_changed {
            self.save_render_defaults();
        }
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
        let palette = theme::palette(light, self.accent);
        let screen = ctx.content_rect();
        let center = screen.center();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("rve-startup"),
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
        painter.rect_filled(card, 18.0, palette.surface.gamma_multiply(opacity));
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
            palette.text.gamma_multiply(opacity),
        );
        painter.text(
            center + Vec2::new(0.0, 40.0),
            egui::Align2::CENTER_CENTER,
            "Starting your local engine…",
            egui::FontId::proportional(12.0),
            palette.secondary.gamma_multiply(opacity),
        );
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

impl eframe::App for NativeApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let Some(engine) = self._engine.as_mut() else {
            return;
        };

        // A failed second instance must never shut down the first instance's
        // loopback service. Only the child that is still running is ours.
        if engine.try_wait().ok().flatten().is_some() {
            return;
        }

        let client = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(2)))
            .build()
            .into();
        let _ = api_post(&client, "/api/app/quit", &json!({}));

        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline && engine.try_wait().ok().flatten().is_none() {
            thread::sleep(Duration::from_millis(25));
        }
        if engine.try_wait().ok().flatten().is_none() {
            let _ = engine.kill();
            let _ = engine.wait();
        }
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        self.consume_events(&ctx);
        if let Some(path) = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        }) {
            self.input_path = path.to_string_lossy().into_owned();
            if let Some(parent) = path.parent() {
                self.output_folder = parent.to_string_lossy().into_owned();
            }
            self.input_info = json!({"status": "inspecting"});
            self.post("/api/video/inspect", json!({"path": self.input_path}));
        }
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
                    .fill(theme::palette(light, self.accent).sidebar)
                    .stroke(Stroke::new(1.0, border_color(light)))
                    .inner_margin(if compact_sidebar { 9 } else { 16 }),
            )
            .show(root_ui, |ui| {
                ui.add_space(4.0);
                if compact_sidebar {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        Self::brand_mark(ui, 40.0);
                    });
                } else {
                    ui.horizontal(|ui| {
                        Self::brand_mark(ui, 42.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("REAL VIDEO")
                                    .size(11.0)
                                    .strong()
                                    .color(theme::palette(light, self.accent).text),
                            );
                            ui.label(
                                RichText::new("ENHANCER")
                                    .size(9.0)
                                    .color(secondary_text_color(light)),
                            );
                        });
                    });
                }
                ui.add_space(if compact_sidebar {
                    20.0
                } else {
                    theme::space::XXL
                });
                if !compact_sidebar {
                    ui.label(
                        RichText::new("WORKSPACE")
                            .size(10.0)
                            .strong()
                            .color(secondary_text_color(light)),
                    );
                    ui.add_space(8.0);
                }
                let first_navigation_row = ui.cursor().min.y;
                let motion_duration = if self.reduce_motion {
                    0.0
                } else {
                    theme::motion::NAV_SECONDS
                };
                let active_index = ctx.animate_value_with_time(
                    egui::Id::unique("navigation-selection"),
                    self.page as f32,
                    motion_duration,
                );
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
                    let hover = ctx.animate_bool_with_time(
                        egui::Id::unique(("navigation-hover", index)),
                        response.hovered(),
                        if self.reduce_motion {
                            0.0
                        } else {
                            theme::motion::HOVER_SECONDS
                        },
                    );
                    let fill = if selected {
                        theme_accent.gamma_multiply(if light { 0.12 } else { 0.10 })
                    } else if response.hovered() {
                        ui.visuals().widgets.hovered.bg_fill.gamma_multiply(hover)
                    } else {
                        Color32::TRANSPARENT
                    };
                    let painter = ui.painter().clone();
                    painter.rect_filled(rect, 8.0, fill);
                    let active_center = first_navigation_row + active_index * 47.0 + 22.0;
                    if active_center >= rect.top() - 10.0 && active_center <= rect.bottom() + 10.0 {
                        painter.rect_filled(
                            egui::Rect::from_center_size(
                                egui::pos2(rect.left() + 2.0, active_center),
                                Vec2::new(3.0, 22.0),
                            ),
                            1.5,
                            theme_accent,
                        );
                    }
                    let icon_color = if selected {
                        theme_accent
                    } else {
                        secondary_text_color(light)
                    };
                    let icon_center = if compact_sidebar {
                        rect.center()
                    } else {
                        egui::pos2(rect.left() + 21.0, rect.center().y)
                    };
                    ui.put(
                        egui::Rect::from_center_size(icon_center, Vec2::splat(22.0)),
                        icons::styled(icon, icon_color, 18.0),
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
                                theme::palette(light, self.accent).success
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
                    .fill(theme::palette(light, self.accent).canvas)
                    .inner_margin(egui::Margin::symmetric(
                        if compact_sidebar { 16 } else { 30 },
                        theme::space::XL as i8,
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
    }
}

fn main() -> eframe::Result<()> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../docs/assets/rve-logo.png"))
        .unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("REAL Video Enhancer")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([840.0, 620.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "REAL Video Enhancer",
        options,
        Box::new(|ctx| Ok(Box::new(NativeApp::new(ctx)))),
    )
}
