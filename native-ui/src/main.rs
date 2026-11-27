#![windows_subsystem = "windows"]

use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke, Vec2};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU16, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(windows)]
use tray_icon::{
    Icon as TrayIconImage, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
mod prefs;
mod ui;
mod views;
mod worker;
use prefs::{HistoryEntry, Prefs, RenderQueue};
use ui::progress::{EtaTracker, Timing};
use ui::{components, icons, theme};
use views::{CommandPalette, ModelBrowser, Rate};
use worker::{ApiBridge, ApiCommand};

static API_PORT: AtomicU16 = AtomicU16::new(51837);
const APP_VERSION: &str = match option_env!("RVE_APP_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
const PAGES: [(&str, &str); 8] = [
    ("Home", "home"),
    ("Studio", "process"),
    ("Models", "models"),
    ("Community", "library"),
    ("Benchmarks", "benchmark"),
    ("Runtime", "runtime"),
    ("Python", "python"),
    ("Settings", "settings"),
];
const PROCESS_PAGE: usize = 1;
const BENCHMARK_PAGE: usize = 4;
const RUNTIME_PAGE: usize = 5;
const SETTINGS_PAGE: usize = 7;
const PROCESS_COLUMN_THRESHOLD: f32 = 880.0;
const RUNTIME_METRICS_COLUMN_THRESHOLD: f32 = 760.0;
const HOME_HERO_COLUMN_THRESHOLD: f32 = 780.0;
const HOME_STATUS_THREE_COLUMN_THRESHOLD: f32 = 900.0;

fn can_start_model_download(connected: bool, active: bool, busy: bool) -> bool {
    connected && !active && !busy
}

fn backend_is_installed(available: &Value, selected: &str) -> bool {
    available.as_array().is_some_and(|backends| {
        backends
            .iter()
            .filter_map(Value::as_str)
            .any(|backend| backend.trim().eq_ignore_ascii_case(selected.trim()))
    })
}

fn encoded_query_value(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size:.0} {}", UNITS[unit])
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

fn pending_post_after_event(pending: bool, is_post: bool) -> bool {
    pending && !is_post
}

fn settings_uses_two_columns(available_width: f32) -> bool {
    available_width >= 960.0
}

fn process_uses_two_columns(available_width: f32) -> bool {
    available_width >= PROCESS_COLUMN_THRESHOLD
}

fn runtime_metrics_use_two_columns(available_width: f32) -> bool {
    available_width >= RUNTIME_METRICS_COLUMN_THRESHOLD
}

fn home_hero_uses_two_columns(available_width: f32) -> bool {
    available_width >= HOME_HERO_COLUMN_THRESHOLD
}

fn home_status_uses_three_columns(available_width: f32) -> bool {
    available_width >= HOME_STATUS_THREE_COLUMN_THRESHOLD
}

fn render_preview_is_available(status: &Value) -> bool {
    matches!(
        status["status"].as_str(),
        Some("preparing" | "running" | "paused")
    ) && status["preview"]["available"].as_bool() == Some(true)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Info,
    Success,
    Error,
}

struct AppToast {
    title: String,
    detail: String,
    kind: ToastKind,
    expires_at: Instant,
}

#[derive(Clone, Copy)]
enum TrayCommand {
    Open,
    Quit,
}

fn active_task_from(
    render: &str,
    download_active: bool,
    runtime: &str,
    storage: &str,
    update: &str,
    anime: &str,
) -> Option<&'static str> {
    if matches!(render, "preparing" | "running" | "paused" | "stopping") {
        Some("A video render")
    } else if download_active {
        Some("A model download")
    } else if runtime == "running" {
        Some("Runtime setup")
    } else if matches!(storage, "starting" | "installing") {
        Some("Model storage setup")
    } else if matches!(update, "downloading" | "installing") {
        Some("An app update")
    } else if anime == "running" {
        Some("AnimeScripter setup")
    } else {
        None
    }
}

fn task_started_title(task: &str) -> &'static str {
    match task {
        "A video render" => "Render started",
        "A model benchmark" => "Benchmark started",
        "A model download" => "Model download started",
        "Runtime setup" => "Runtime setup started",
        "Package cache cleanup" => "Cache cleanup started",
        "Model storage setup" => "Model storage setup started",
        "An app update" => "Update started",
        "AnimeScripter setup" => "AnimeScripter setup started",
        _ => "Work started",
    }
}

fn task_started_detail(task: &str) -> &'static str {
    match task {
        "A video render" => "Your video is being processed.",
        "A model benchmark" => "Measuring the selected models on this PC.",
        "A model download" => "Downloading model files to your selected storage.",
        "Runtime setup" => "Preparing the app-managed Python and inference runtime.",
        "Package cache cleanup" => {
            "Removing unused package downloads while keeping the installed runtime."
        }
        "Model storage setup" => "Preparing the selected model storage location.",
        "An app update" => "Downloading or installing the latest Windows release.",
        "AnimeScripter setup" => "Installing and checking the optional AnimeScripter engine.",
        _ => "The app is working on your request.",
    }
}

#[cfg(windows)]
fn create_tray(ctx: &egui::Context) -> (Option<TrayIcon>, Receiver<TrayCommand>, Option<String>) {
    let (send, receive) = mpsc::channel();
    let open = MenuItem::with_id("rve-open", "Open REAL Video Enhancer", true, None);
    let quit = MenuItem::with_id("rve-quit", "Quit when work finishes", true, None);
    let menu = Menu::new();
    let separator = PredefinedMenuItem::separator();
    if let Err(error) = menu.append_items(&[&open, &separator, &quit]) {
        return (
            None,
            receive,
            Some(format!("Could not create the tray menu: {error}")),
        );
    }

    let open_id = open.id().clone();
    let quit_id = quit.id().clone();
    let menu_send = send.clone();
    let menu_ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = if event.id == open_id {
            Some(TrayCommand::Open)
        } else if event.id == quit_id {
            Some(TrayCommand::Quit)
        } else {
            None
        };
        if let Some(command) = command {
            let _ = menu_send.send(command);
            if matches!(command, TrayCommand::Open) {
                menu_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                menu_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            menu_ctx.request_repaint();
        }
    }));

    let click_send = send;
    let click_ctx = ctx.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                ..
            }
        ) {
            let _ = click_send.send(TrayCommand::Open);
            click_ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            click_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            click_ctx.request_repaint();
        }
    }));

    let image =
        match eframe::icon_data::from_png_bytes(include_bytes!("../../docs/assets/rve-logo.png")) {
            Ok(image) => image,
            Err(error) => {
                return (
                    None,
                    receive,
                    Some(format!("Could not load the tray icon: {error}")),
                );
            }
        };
    let image = match TrayIconImage::from_rgba(image.rgba, image.width, image.height) {
        Ok(image) => image,
        Err(error) => {
            return (
                None,
                receive,
                Some(format!("Could not prepare the tray icon: {error}")),
            );
        }
    };
    match TrayIconBuilder::new()
        .with_tooltip("REAL Video Enhancer")
        .with_icon(image)
        .with_menu(Box::new(menu))
        .build()
    {
        Ok(icon) => (Some(icon), receive, None),
        Err(error) => (
            None,
            receive,
            Some(format!("Could not create the system tray icon: {error}")),
        ),
    }
}

#[cfg(not(windows))]
fn create_tray(_: &egui::Context) -> (Receiver<TrayCommand>, Option<String>) {
    let (_, receive) = mpsc::channel();
    (
        receive,
        Some("The system tray is currently supported on Windows only.".to_owned()),
    )
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

fn api_get_image(client: &ureq::Agent, endpoint: &str) -> Result<Vec<u8>, String> {
    client
        .get(format!(
            "http://127.0.0.1:{}{endpoint}",
            API_PORT.load(Ordering::Relaxed)
        ))
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_vec()
        .map_err(|error| error.to_string())
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
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

fn start_engine(model_storage: &str) -> Result<Option<Child>, String> {
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
        .env("RVE_AUTO_SETUP_MODELS", "0")
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

fn apply_theme(ctx: &egui::Context, accent: Color32, appearance: &str, reduce_motion: bool) {
    theme::apply(ctx, accent, appearance, reduce_motion);
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
    #[cfg(windows)]
    _tray_icon: Option<TrayIcon>,
    tray_events: Receiver<TrayCommand>,
    tray_error: Option<String>,
    close_confirmation: bool,
    destructive_confirmation: Option<String>,
    managed_data_message: Option<String>,
    managed_data_kind: Option<String>,
    managed_data_failed: bool,
    managed_data_pending: bool,
    quit_when_idle: bool,
    page: usize,
    prefs: Prefs,
    queue: RenderQueue,
    eta: EtaTracker,
    timings: HashMap<&'static str, Timing>,
    download_rate: Rate,
    /// Frames the running render is expected to write.
    render_total: Option<f64>,
    settings_tab: usize,
    model_browser: ModelBrowser,
    command: CommandPalette,
    window_handle: Option<isize>,
    material_active: bool,
    logo: Option<egui::TextureHandle>,
    shown_page: usize,
    page_changed: Instant,
    render_input: String,
    failed_update: Option<String>,
    connected: bool,
    engine_error: Option<String>,
    input_info: Value,
    source_preview: Option<egui::TextureHandle>,
    render_preview: Option<egui::TextureHandle>,
    source_preview_path: String,
    source_preview_loading: bool,
    source_preview_error: Option<String>,
    bootstrap: Value,
    models: Value,
    render: Value,
    download: Value,
    install: Value,
    anime: Value,
    library: Value,
    library_loading: bool,
    library_download_refresh_pending: bool,
    library_images: HashMap<String, egui::TextureHandle>,
    library_image_loading: HashSet<String>,
    library_image_failed: HashSet<String>,
    library_image_order: VecDeque<String>,
    library_visible_count: usize,
    footprint: Value,
    footprint_loading: bool,
    python: Value,
    storage_status: Value,
    benchmarks: Value,
    update: Value,
    update_token_input: String,
    update_auth_message: String,
    update_install_started: bool,
    saved_settings: Value,
    encoding: Value,
    model_storage: String,
    first_setup: bool,
    backend: String,
    render_engine: String,
    anime_options: Value,
    library_search: String,
    library_category: String,
    benchmark_search: String,
    benchmark_category: String,
    benchmark_resolutions: HashMap<String, bool>,
    benchmark_frames: usize,
    model_picker_search: HashMap<String, String>,
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
    toast: Option<AppToast>,
    last_active_task: Option<&'static str>,
}

impl NativeApp {
    fn new(ctx: &eframe::CreationContext<'_>) -> Self {
        #[cfg(windows)]
        let (tray_icon, tray_events, tray_error) = create_tray(&ctx.egui_ctx);
        #[cfg(not(windows))]
        let (tray_events, tray_error) = create_tray(&ctx.egui_ctx);
        let prefs = Prefs::load();
        let model_storage = saved_model_storage();
        let first_setup = !model_storage_configured();
        egui_extras::install_image_loaders(&ctx.egui_ctx);
        theme::install_fonts(&ctx.egui_ctx);
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
        let mut app = Self {
            api: worker::start(ctx.egui_ctx.clone(), model_storage.clone()),
            #[cfg(windows)]
            _tray_icon: tray_icon,
            tray_events,
            tray_error,
            close_confirmation: false,
            destructive_confirmation: None,
            managed_data_message: None,
            managed_data_kind: None,
            managed_data_failed: false,
            managed_data_pending: false,
            quit_when_idle: false,
            page: PROCESS_PAGE,
            prefs,
            queue: RenderQueue::default(),
            eta: EtaTracker::default(),
            timings: HashMap::new(),
            download_rate: Rate::default(),
            render_total: None,
            settings_tab: if first_setup { 2 } else { 0 },
            model_browser: ModelBrowser::default(),
            command: CommandPalette::default(),
            window_handle: window_handle(ctx),
            material_active: false,
            logo: None,
            shown_page: PROCESS_PAGE,
            page_changed: Instant::now(),
            render_input: String::new(),
            failed_update: None,
            connected: false,
            engine_error: None,
            input_info: Value::Null,
            source_preview: None,
            render_preview: None,
            source_preview_path: String::new(),
            source_preview_loading: false,
            source_preview_error: None,
            bootstrap: Value::Null,
            models: Value::Null,
            render: Value::Null,
            download: Value::Null,
            install: Value::Null,
            anime: Value::Null,
            library: Value::Null,
            library_loading: false,
            library_download_refresh_pending: false,
            library_images: HashMap::new(),
            library_image_loading: HashSet::new(),
            library_image_failed: HashSet::new(),
            library_image_order: VecDeque::new(),
            library_visible_count: 12,
            footprint: Value::Null,
            footprint_loading: false,
            python: Value::Null,
            storage_status: Value::Null,
            benchmarks: Value::Null,
            update: Value::Null,
            update_token_input: String::new(),
            update_auth_message: String::new(),
            update_install_started: false,
            saved_settings: json!({}),
            encoding: default_encoding(),
            model_storage,
            first_setup,
            backend: "pytorch".to_owned(),
            render_engine: "rve".to_owned(),
            anime_options: json!({"methods": {"upscale": "shufflecugan", "interpolate": "rife4.6", "restore": "anime1080fixer"}}),
            library_search: String::new(),
            library_category: "all".to_owned(),
            benchmark_search: String::new(),
            benchmark_category: "all".to_owned(),
            benchmark_resolutions: ["720p", "1080p", "4K"]
                .into_iter()
                .map(|resolution| (resolution.to_owned(), resolution == "720p"))
                .chain(
                    ["480p", "1440p"]
                        .into_iter()
                        .map(|resolution| (resolution.to_owned(), false)),
                )
                .collect(),
            benchmark_frames: 48,
            model_picker_search: HashMap::new(),
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
            toast: None,
            last_active_task: None,
        };
        app.apply_look(&ctx.egui_ctx);
        if let Some(page) = std::env::var("RVE_START_PAGE")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|page| *page < PAGES.len())
        {
            app.page = page;
        }
        app
    }

    fn restore_backend_settings(&mut self, settings: &Value) {
        self.saved_settings = if settings.is_object() {
            settings.clone()
        } else {
            json!({})
        };
        let advanced = &self.saved_settings["advanced"];
        let configured_backend = self.saved_settings["backend"]
            .as_str()
            .unwrap_or("pytorch")
            .trim();
        self.backend = if configured_backend.eq_ignore_ascii_case("auto") {
            "pytorch".to_owned()
        } else {
            configured_backend.to_ascii_lowercase()
        };
        self.render_engine = advanced["render_engine"]
            .as_str()
            .unwrap_or("rve")
            .to_owned();
        if let Some(options) = self.saved_settings["animescripter"].as_object() {
            self.anime_options = Value::Object(options.clone());
        }
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
        if self.input_path.is_empty()
            && let Some(output_folder) = advanced["output_folder_location"].as_str()
        {
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

    fn render_defaults_snapshot(&mut self) -> Value {
        if !self.saved_settings.is_object() {
            self.saved_settings = json!({});
        }
        if !self.saved_settings["advanced"].is_object() {
            self.saved_settings["advanced"] = json!({});
        }
        self.saved_settings["backend"] = json!(self.backend);
        self.saved_settings["animescripter"] = self.anime_options.clone();
        self.saved_settings["toggles"] = json!(self.toggles);
        self.saved_settings["encoding"] = self.encoding.clone();
        let advanced = &mut self.saved_settings["advanced"];
        advanced["output_folder_location"] = json!(self.output_folder);
        advanced["render_engine"] = json!(self.render_engine);
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
        self.saved_settings.clone()
    }

    fn save_render_defaults(&mut self) {
        let settings = self.render_defaults_snapshot();
        self.post("/api/settings", json!({"settings": settings}));
    }

    fn get(&self, endpoint: &str) {
        let _ = self.api.send.send(ApiCommand::Get(endpoint.to_owned()));
    }

    fn request_source_preview(&mut self) {
        let path = self.input_path.trim();
        if path.is_empty() {
            self.source_preview = None;
            self.source_preview_path.clear();
            self.source_preview_loading = false;
            self.source_preview_error = None;
            return;
        }
        self.source_preview = None;
        self.source_preview_path = path.to_owned();
        self.source_preview_loading = true;
        self.source_preview_error = None;
        self.get(&format!(
            "/api/video/first_frame?path={}",
            encode_query_component(path)
        ));
    }

    /// A POST that neither waits for nor blocks other actions.
    fn query(&self, endpoint: &str, payload: Value) {
        let _ = self
            .api
            .send
            .send(ApiCommand::Query(endpoint.to_owned(), payload));
    }

    fn post(&mut self, endpoint: &str, payload: Value) {
        // Stopping and pausing must work even while another request is pending.
        if matches!(
            endpoint,
            "/api/actions/cancel" | "/api/render/stop" | "/api/render/pause"
        ) {
            self.query(endpoint, payload);
            return;
        }
        if self.busy {
            self.show_toast(
                "One moment",
                "The engine is still finishing the previous action.",
                ToastKind::Info,
            );
            return;
        }
        let task = match endpoint {
            "/api/render/start" => Some((
                "A video render",
                "Render started",
                "Preparing your video for processing.",
            )),
            "/api/benchmarks/start" => Some((
                "A model benchmark",
                "Benchmark started",
                "Measuring the selected models on this PC.",
            )),
            "/api/models/download" | "/api/library/download" => Some((
                "A model download",
                "Model download started",
                "Downloading model files to your selected storage.",
            )),
            "/api/startup/install" => Some((
                "Runtime setup",
                "Runtime setup started",
                "Preparing the app-managed Python and inference runtime.",
            )),
            "/api/storage/slim" => Some((
                "Runtime cleanup",
                "Slimming the runtime",
                "Removing build-only PyTorch files.",
            )),
            "/api/storage/prune-cache" => Some((
                "Package cache cleanup",
                "Cache cleanup started",
                "Removing unused package downloads. Installed Python packages stay in place.",
            )),
            "/api/animescripter/install" => Some((
                "AnimeScripter setup",
                "AnimeScripter setup started",
                "Installing and checking the optional AnimeScripter engine.",
            )),
            "/api/storage/configure" => Some((
                "Model storage setup",
                "Model storage setup started",
                "Preparing your chosen Windows or WSL model library.",
            )),
            "/api/app/update/download" => Some((
                "An app update",
                "Update download started",
                "Downloading the latest Windows installer.",
            )),
            "/api/data/delete" => Some((
                "Managed files cleanup",
                "Cleanup started",
                "Removing the selected app-managed files.",
            )),
            _ => None,
        };
        if let Some((_active_task, title, detail)) = task {
            self.show_toast(title, detail, ToastKind::Info);
        }
        self.busy = true;
        if self
            .api
            .send
            .send(ApiCommand::Post(endpoint.to_owned(), payload))
            .is_err()
        {
            self.busy = false;
        }
    }

    fn show_toast(&mut self, title: impl Into<String>, detail: impl Into<String>, kind: ToastKind) {
        let title = title.into();
        let now = Instant::now();
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| toast.title == title && toast.expires_at > now)
        {
            return;
        }
        self.toast = Some(AppToast {
            title,
            detail: detail.into(),
            kind,
            expires_at: now + Duration::from_secs(5),
        });
    }

    fn check_task_toast(&mut self) {
        let current = self
            .active_task()
            .filter(|task| *task != "An engine operation" && *task != "Local engine startup");
        match (self.last_active_task, current) {
            (None, Some(task)) => {
                self.last_active_task = Some(task);
                self.show_toast(
                    task_started_title(task),
                    task_started_detail(task),
                    ToastKind::Info,
                );
            }
            (Some(previous), None) => {
                self.last_active_task = None;
                let (title, detail, kind) = self.task_finished_toast(previous);
                self.show_toast(title, detail, kind);
            }
            (_, Some(task)) => self.last_active_task = Some(task),
            (None, None) => {}
        }
    }

    fn task_finished_toast(&self, task: &str) -> (String, String, ToastKind) {
        let (status, detail, error) = match task {
            "A video render" => (
                self.render["status"].as_str().unwrap_or_default(),
                self.render["detail"]
                    .as_str()
                    .or_else(|| self.render["message"].as_str()),
                self.render["error"].as_str(),
            ),
            "A model benchmark" => (
                self.benchmarks["status"].as_str().unwrap_or_default(),
                self.benchmarks["detail"].as_str(),
                self.benchmarks["error"].as_str(),
            ),
            "A model download" => (
                self.download["status"].as_str().unwrap_or_default(),
                self.download["detail"].as_str(),
                self.download["error"].as_str(),
            ),
            "Model storage setup" => (
                self.storage_status["status"].as_str().unwrap_or_default(),
                self.storage_status["detail"].as_str(),
                self.storage_status["error"].as_str(),
            ),
            "Runtime setup" | "AnimeScripter setup" => (
                if task == "AnimeScripter setup" {
                    self.anime["status"].as_str().unwrap_or_default()
                } else {
                    self.install["status"].as_str().unwrap_or_default()
                },
                self.install["detail"]
                    .as_str()
                    .or_else(|| self.anime["detail"].as_str()),
                self.install["error"]
                    .as_str()
                    .or_else(|| self.anime["error"].as_str()),
            ),
            "Package cache cleanup" => (
                self.install["status"].as_str().unwrap_or_default(),
                self.install["detail"].as_str(),
                self.install["error"].as_str(),
            ),
            "An app update" => (
                self.update["status"].as_str().unwrap_or_default(),
                self.update["detail"].as_str(),
                self.update["error"].as_str(),
            ),
            _ => ("", None, None),
        };
        if matches!(status, "error" | "failed") || error.is_some_and(|value| !value.is_empty()) {
            (
                "Task needs attention".to_owned(),
                error.or(detail).unwrap_or("The task failed.").to_owned(),
                ToastKind::Error,
            )
        } else if matches!(status, "cancelled" | "canceled" | "stopped") {
            (
                "Task cancelled".to_owned(),
                detail.unwrap_or("The operation was stopped.").to_owned(),
                ToastKind::Info,
            )
        } else {
            (
                "Task complete".to_owned(),
                detail
                    .unwrap_or("The operation finished successfully.")
                    .to_owned(),
                ToastKind::Success,
            )
        }
    }

    fn start_recommended_runtime_setup(&mut self) {
        if self.busy {
            return;
        }
        self.install = json!({
            "status": "running",
            "progress": 1,
            "stage": "Preparing runtime",
            "detail": "Starting managed runtime setup.",
            "output": []
        });
        self.python = json!({
            "python_available": false,
            "python_managed": false,
            "python_detail": "Setting up the app-managed Python runtime."
        });
        self.post("/api/startup/install", json!({"backend": "pytorch"}));
    }

    fn active_task(&self) -> Option<&'static str> {
        if self.benchmarks["status"].as_str() == Some("running") {
            return Some("A model benchmark");
        }
        if !self.connected && self.engine_error.is_none() {
            return Some("Local engine startup");
        }
        if self.busy {
            return Some("An engine operation");
        }
        if self.install["status"].as_str() == Some("running")
            && self.install["stage"].as_str() == Some("Pruning unused package cache")
        {
            return Some("Package cache cleanup");
        }
        active_task_from(
            self.render["status"].as_str().unwrap_or_default(),
            self.download["active"].as_bool() == Some(true),
            self.install["status"].as_str().unwrap_or_default(),
            self.storage_status["status"].as_str().unwrap_or_default(),
            self.update["status"].as_str().unwrap_or_default(),
            self.anime["status"].as_str().unwrap_or_default(),
        )
    }

    fn other_task_active(&self) -> bool {
        self.benchmarks["status"].as_str() == Some("running")
            || (!self.connected && self.engine_error.is_none())
            || self.busy
            || active_task_from(
                self.render["status"].as_str().unwrap_or_default(),
                self.download["active"].as_bool() == Some(true),
                self.install["status"].as_str().unwrap_or_default(),
                self.storage_status["status"].as_str().unwrap_or_default(),
                "",
                self.anime["status"].as_str().unwrap_or_default(),
            )
            .is_some()
    }

    fn install_downloaded_update(&mut self, ctx: &egui::Context) {
        let Some(path) = self.update["installer_path"].as_str().map(str::to_owned) else {
            self.update["status"] = json!("error");
            self.update["detail"] = json!(
                "The downloaded update has no installer path. Check for updates and download it again."
            );
            return;
        };

        let result = Command::new(&path)
            .args(["/VERYSILENT", "/NORESTART", "/CLOSEAPPLICATIONS", "/SP-"])
            .current_dir(Path::new(&path).parent().unwrap_or_else(|| Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match result {
            Ok(_installer) => {
                self.update_install_started = true;
                self.update["status"] = json!("installing");
                self.update["detail"] = json!(
                    "Update downloaded. Installing quietly and reopening REAL Video Enhancer…"
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(error) => {
                // The engine keeps reporting "downloaded"; do not retry forever.
                self.failed_update = Some(path);
                self.show_toast(
                    "Update could not start",
                    error.to_string(),
                    ToastKind::Error,
                );
            }
        }
    }

    fn tray_available(&self) -> bool {
        #[cfg(windows)]
        {
            self._tray_icon.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        // Nothing is visible, so give the picture memory back.
        self.render_preview = None;
        self.library_images.clear();
        self.library_image_order.clear();
        #[cfg(windows)]
        if let Some(icon) = self._tray_icon.as_ref() {
            let activity = self.active_task().unwrap_or("Work");
            let _ = icon.set_tooltip(Some(format!(
                "REAL Video Enhancer · {activity} in background"
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.api.hidden.store(true, Ordering::Relaxed);
        }
        #[cfg(not(windows))]
        let _ = ctx;
    }

    fn process_tray_commands(&mut self, ctx: &egui::Context) {
        while let Ok(command) = self.tray_events.try_recv() {
            match command {
                TrayCommand::Open => {
                    self.api.hidden.store(false, Ordering::Relaxed);
                    self.close_confirmation = false;
                    #[cfg(windows)]
                    if let Some(icon) = self._tray_icon.as_ref() {
                        let _ = icon.set_tooltip(Some("REAL Video Enhancer"));
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayCommand::Quit => {
                    if self.active_task().is_some() {
                        self.close_confirmation = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    } else {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        if self.quit_when_idle && self.active_task().is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn consume_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.api.receive.try_recv() {
            if let Some(image) = event.image {
                self.receive_image(ctx, &event.endpoint, image);
                continue;
            }
            self.busy = pending_post_after_event(self.busy, event.is_post);
            if event.endpoint.starts_with("/api/models/inventory") {
                self.model_browser.received(event.value);
                continue;
            }
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
                        self.models = value;
                        self.drop_unknown_models();
                    }
                    "/api/render/status" => self.receive_render_status(ctx, value),
                    "/api/render/start" => {
                        self.render["status"] = json!("preparing");
                        self.render["error"] = Value::Null;
                        self.get("/api/render/status");
                    }
                    "/api/models/download_status" => {
                        let was_active = self.download["active"].as_bool() == Some(true);
                        self.download = value.clone();
                        self.download_rate.sample(
                            value["bytes_received"].as_u64().unwrap_or(0),
                            Instant::now(),
                        );
                        if was_active && value["active"].as_bool() != Some(true) {
                            self.model_browser.invalidate();
                            self.get(&format!("/api/models?backend={}", self.backend));
                        }
                        if self.library_download_refresh_pending
                            && value["active"].as_bool() != Some(true)
                        {
                            if value["status"].as_str() == Some("completed") {
                                self.get(&format!("/api/models?backend={}", self.backend));
                            }
                            self.library_download_refresh_pending = false;
                        }
                    }
                    "/api/models/delete" => {
                        let detail = value["detail"].as_str().unwrap_or("Model removed.");
                        self.show_toast("Model removed", detail.to_owned(), ToastKind::Success);
                        self.model_browser.invalidate();
                        self.get(&format!("/api/models?backend={}", self.backend));
                    }
                    "/api/actions/import_custom_models" => {
                        self.model_browser.invalidate();
                        self.get(&format!("/api/models?backend={}", self.backend));
                    }
                    "/api/dialog/select_inputs" => {
                        let added = value["paths"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .filter(|path| self.queue.add(path))
                            .count();
                        if added > 0 {
                            self.show_toast(
                                "Added to queue",
                                format!("{added} video(s) are waiting in the Studio."),
                                ToastKind::Success,
                            );
                        } else if let Some(detail) = value["detail"].as_str()
                            && value["path"].as_str().is_some_and(|path| !path.is_empty())
                        {
                            self.show_toast("Nothing added", detail.to_owned(), ToastKind::Info);
                        }
                    }
                    "/api/library/download" => {
                        self.library_download_refresh_pending = true;
                        self.download = value;
                    }
                    path if path.starts_with("/api/library/models") => {
                        self.library_loading = false;
                        self.library = value;
                    }
                    "/api/storage/footprint" => {
                        self.footprint_loading = false;
                        self.footprint = value;
                    }
                    "/api/startup/install_status" => {
                        let status = value["status"].as_str().unwrap_or_default().to_owned();
                        let previous = self.install["status"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned();
                        self.install = value;
                        if previous != status
                            && matches!(status.as_str(), "completed" | "error" | "cancelled")
                        {
                            self.get("/api/python/info");
                            self.get("/api/bootstrap");
                            self.get("/api/storage/footprint");
                        }
                    }
                    "/api/animescripter/status" => self.anime = value,
                    "/api/python/info" => self.python = value,
                    "/api/video/inspect" => {
                        // A newer choice supersedes an answer still in flight.
                        if let Some(path) = value["path"].as_str()
                            && !same_file(path, &self.input_path)
                        {
                            continue;
                        }
                        self.input_info = value;
                        if self.input_info["error"].is_null() {
                            self.prefs.remember_recent(self.input_path.trim());
                            self.request_source_preview();
                        } else {
                            self.source_preview = None;
                            self.source_preview_loading = false;
                            self.source_preview_error = None;
                        }
                    }
                    "/api/python/pip" => {
                        let command = value["command"].as_str().unwrap_or("pip");
                        let output = value["output"]
                            .as_str()
                            .unwrap_or("No output was returned.");
                        self.logs.push(format!("$ {command}\n{output}"));
                    }
                    "/api/storage/status" => self.storage_status = value,
                    "/api/benchmarks/status"
                    | "/api/benchmarks/start"
                    | "/api/benchmarks/cancel" => {
                        self.benchmarks = value;
                    }
                    "/api/data/delete" => {
                        let detail = value["detail"]
                            .as_str()
                            .unwrap_or("Managed files removed.")
                            .to_owned();
                        self.managed_data_message = Some(detail.clone());
                        self.managed_data_failed = value["ok"].as_bool() == Some(false);
                        self.managed_data_pending = false;
                        self.logs.push(detail);
                        let toast_detail = self
                            .managed_data_message
                            .clone()
                            .unwrap_or_else(|| "Managed app data cleanup finished.".to_owned());
                        self.show_toast(
                            if self.managed_data_failed {
                                "Cleanup failed"
                            } else {
                                "Cleanup complete"
                            },
                            toast_detail,
                            if self.managed_data_failed {
                                ToastKind::Error
                            } else {
                                ToastKind::Success
                            },
                        );
                        self.get(&format!("/api/models?backend={}", self.backend));
                        self.get("/api/python/info");
                        self.get("/api/storage/status");
                        self.get("/api/startup/install_status");
                        if self.managed_data_kind.as_deref() == Some("python") {
                            self.get("/api/bootstrap");
                        }
                    }
                    "/api/app/update/status"
                    | "/api/app/update/check"
                    | "/api/app/update/download" => self.update = value,
                    "/api/app/update/auth" => {
                        self.update_auth_message = value["detail"]
                            .as_str()
                            .or_else(|| value["error"].as_str())
                            .unwrap_or("GitHub updater access could not be saved.")
                            .to_owned();
                        self.update["has_github_token"] =
                            json!(value["configured"].as_bool().unwrap_or(false));
                    }
                    "/api/dialog/select_input" => {
                        let selected = value["path"].as_str().unwrap_or_default().trim();
                        // Cancelling the picker leaves the current source and its
                        // preview intact instead of silently clearing the project.
                        if !selected.is_empty() {
                            self.input_path = selected.to_owned();
                            self.input_info = value["video_info"].clone();
                            self.prefs.remember_recent(&self.input_path);
                            self.request_source_preview();
                            if let Some(parent) = Path::new(&self.input_path).parent() {
                                self.output_folder = parent.to_string_lossy().into_owned();
                            }
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
                    if event.is_post
                        || event.endpoint.starts_with("/api/dialog/")
                        || event.endpoint.starts_with("/api/actions/")
                    {
                        self.show_toast(
                            "Could not complete that action",
                            error.clone(),
                            ToastKind::Error,
                        );
                    }
                    if event.endpoint == "/api/render/start" {
                        self.render["status"] = json!("error");
                        self.render["error"] = json!(error);
                    }
                    if event.endpoint == "/api/data/delete" {
                        self.managed_data_message = Some(format!("Delete failed: {error}"));
                        self.managed_data_failed = true;
                        self.managed_data_pending = false;
                    }
                    if event.endpoint == "/api/bootstrap" || event.endpoint == "/api/connection" {
                        self.connected = false;
                        self.engine_error = Some(error.clone());
                    }
                    if event.endpoint == "/api/python/info" {
                        self.python = json!({
                            "python_available": false,
                            "python_managed": false,
                            "python_detail": format!("Could not query the app-managed Python environment: {error}"),
                            "error": error
                        });
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

    fn receive_image(
        &mut self,
        ctx: &egui::Context,
        endpoint: &str,
        image: Result<egui::ColorImage, String>,
    ) {
        const THUMBNAIL_LIMIT: usize = 24;
        let options = egui::TextureOptions::LINEAR;
        if let Some(model_id) = endpoint.strip_prefix("/api/library/thumbnail?id=") {
            let model_id = model_id.to_owned();
            self.library_image_loading.remove(&model_id);
            match image {
                Ok(image) => {
                    // Evicted thumbnails are simply requested again when seen.
                    while self.library_images.len() >= THUMBNAIL_LIMIT
                        && let Some(oldest) = self.library_image_order.pop_front()
                    {
                        self.library_images.remove(&oldest);
                    }
                    let texture = ctx.load_texture("rve-thumbnail", image, options);
                    self.library_images.insert(model_id.clone(), texture);
                    self.library_image_order.push_back(model_id);
                }
                Err(_) => {
                    self.library_image_failed.insert(model_id);
                }
            }
        } else if endpoint == "/api/render/preview_frame" {
            if let Ok(image) = image {
                // Update in place instead of allocating a texture per frame.
                match &mut self.render_preview {
                    Some(texture) => texture.set(image, options),
                    None => {
                        self.render_preview =
                            Some(ctx.load_texture("rve-live-preview", image, options));
                    }
                }
            }
        } else if self.source_preview_path == self.input_path.trim() {
            self.source_preview_loading = false;
            match image {
                Ok(image) => {
                    self.source_preview =
                        Some(ctx.load_texture("rve-source-preview", image, options));
                    self.source_preview_error = None;
                }
                Err(error) => self.source_preview_error = Some(error),
            }
        }
    }

    fn receive_render_status(&mut self, ctx: &egui::Context, value: Value) {
        let was_active = matches!(
            self.render["status"].as_str(),
            Some("preparing" | "running" | "paused")
        );
        self.render = value;
        if was_active && self.render["status"] == "completed" {
            if let Some(output) = self.render["output_file"].as_str() {
                self.prefs.remember_render(HistoryEntry {
                    input: self.render_input.clone(),
                    output: output.to_owned(),
                });
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
    }

    /// A model picked for one backend may not exist in another.
    fn drop_unknown_models(&mut self) {
        let models = &self.models;
        self.selected_models.retain(|category, label| {
            models[category.as_str()]
                .as_array()
                .is_none_or(|choices| choices.iter().any(|choice| choice == label.as_str()))
        });
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
        let installed = self.models["model_status"][category].clone();
        let selected = self.selected_models.entry(category.to_owned()).or_default();
        let search = self
            .model_picker_search
            .entry(category.to_owned())
            .or_default();
        components::searchable_select(
            ui,
            egui::Id::unique(("model-picker", category)),
            selected,
            search,
            &options,
            |name| installed[name].as_bool() == Some(true),
        )
    }

    fn start_render(&mut self) {
        if self.busy {
            return;
        }
        let saved_settings = self.render_defaults_snapshot();
        self.render_preview = None;
        self.render_input = self.input_path.clone();
        self.render_total = views::expected_frames(
            &self.input_info,
            (self.toggles.get("interpolate") == Some(&true)).then_some(self.interpolate_factor),
        );
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
                .unwrap_or(true)
        );
        render_settings["merge_subtitles"] = json!(
            self.saved_settings["advanced"]["merge_subtitles"]
                .as_bool()
                .unwrap_or(true)
        );
        render_settings["interpolate_factor"] = json!(self.interpolate_factor);
        render_settings["override_upscale_scale"] = json!(self.upscale_scale);
        let payload = json!({
            "engine": self.render_engine,
            "saved_settings": saved_settings,
            "backend": self.backend,
            "input_path": self.input_path,
            "output_folder": self.output_folder,
            "interpolate_factor": self.interpolate_factor,
            "override_upscale_scale": self.upscale_scale,
            "models": Value::Object(selected),
            "toggles": toggles,
            "settings": render_settings,
            "encoding": self.encoding.clone(),
            "animescripter": self.anime_options.clone()
        });
        self.post("/api/render/start", payload);
    }

    fn page_description(&self) -> &'static str {
        match self.page {
            0 => "Your workspace for cleaner, sharper video.",
            1 => "Choose a source, tune enhancements, and render.",
            2 => "Pick the models you want and download them together.",
            3 => "Search the wider model community and check compatibility.",
            4 => "Compare installed models on your hardware, at real input resolutions.",
            5 => "Set up the inference engines and AnimeScripter.",
            6 => "Inspect the Python engine and its environment.",
            7 => "Personalize appearance, storage, and app updates.",
            _ => "Your workspace for cleaner, sharper video.",
        }
    }

    fn render_blockers(&self) -> Vec<String> {
        let backend_ready = self.render_engine == "animescripter"
            || backend_is_installed(
                &self.bootstrap["system"]["available_backends"],
                &self.backend,
            );
        let active = matches!(
            self.render["status"].as_str(),
            Some("preparing" | "running" | "paused" | "stopping")
        );
        let mut blockers: Vec<String> = Vec::new();
        if !self.connected {
            blockers.push(self.engine_error.as_ref().map_or_else(
                || "The local engine is still starting.".to_owned(),
                |error| format!("The local engine is unavailable: {error}"),
            ));
        }
        if matches!(
            self.install["status"].as_str(),
            Some("running" | "installing" | "downloading")
        ) {
            blockers.push("Wait for runtime setup to finish before rendering.".to_owned());
        }
        if matches!(
            self.storage_status["status"].as_str(),
            Some("starting" | "installing")
        ) {
            blockers.push("Wait for model storage setup to finish before rendering.".to_owned());
        }
        if self.input_path.trim().is_empty() {
            blockers.push("Choose an input video.".to_owned());
        } else if !Path::new(self.input_path.trim()).is_file() {
            blockers.push("The selected input path is not an existing file.".to_owned());
        } else if self.input_info["error"].is_string() {
            blockers.push("Choose a supported video file and inspect it first.".to_owned());
        } else if self.input_info["width"].as_u64().is_none() {
            blockers.push("Inspect the selected video before rendering.".to_owned());
        }
        if !self.output_to_mpv && self.output_folder.trim().is_empty() {
            blockers.push("Choose an output folder.".to_owned());
        }
        if self.render_engine == "animescripter" && self.anime["ready"].as_bool() != Some(true) {
            blockers.push(
                "Install and verify AnimeScripter from the Processing engine card.".to_owned(),
            );
        } else if !backend_ready {
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
        let missing_model = self.render_engine == "rve"
            && enabled.iter().any(|key| {
                self.selected_models
                    .get(*key)
                    .is_none_or(|model| model.is_empty())
            });
        if missing_model {
            blockers.push("Choose a model for each enabled enhancement.".to_owned());
        }
        if active {
            blockers.push("A render is already active.".to_owned());
        }
        if self.benchmarks["status"].as_str() == Some("running") {
            blockers.push("Cancel the active model benchmark before rendering.".to_owned());
        }
        if self.busy {
            blockers.push("Waiting for the engine to finish its current action.".to_owned());
        }
        if self.download["active"].as_bool() == Some(true) {
            blockers
                .push("Wait for the active model download to finish before rendering.".to_owned());
        }
        blockers
    }

    fn render_blocker_action(&mut self, ui: &mut egui::Ui, blocker: &str) {
        let action = if blocker.contains("input video") || blocker.contains("supported video") {
            Some(("Choose source video", 0))
        } else if blocker.contains("output folder") {
            Some(("Choose output folder", 1))
        } else if blocker.contains("processing backend")
            || blocker.contains("AnimeScripter")
            || blocker.contains("backend in Runtime")
            || blocker.contains("runtime setup")
        {
            Some(("Open runtime setup", 2))
        } else if blocker.contains("storage setup") {
            Some(("Open storage settings", 5))
        } else if blocker.contains("download") {
            Some(("View downloads", 3))
        } else if blocker.contains("benchmark") {
            Some(("Open benchmarks", 4))
        } else if blocker.contains("model") || blocker.contains("enhancement") {
            Some(("Choose models", 3))
        } else {
            None
        };
        let Some((label, destination)) = action else {
            return;
        };
        if ui.small_button(label).clicked() {
            match destination {
                0 => self.get("/api/dialog/select_input"),
                1 => self.get("/api/dialog/select_output"),
                2 => self.page = RUNTIME_PAGE,
                4 => self.page = BENCHMARK_PAGE,
                5 => self.page = SETTINGS_PAGE,
                _ => self.page = 2,
            }
        }
    }

    fn processing_engine_card(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.prefs.accent);
        let mut engine_changed = false;
        components::card(ui, "Processing engine", |ui| {
            let mut engine_index = usize::from(self.render_engine == "animescripter");
            if components::segmented(
                ui,
                "engine",
                &["REAL Video Enhancer", "AnimeScripter"],
                &mut engine_index,
            ) {
                self.render_engine = ["rve", "animescripter"][engine_index].to_owned();
                engine_changed = true;
            }
            ui.horizontal_wrapped(|ui| {
                if self.render_engine == "animescripter" {
                    let ready = self.anime["ready"].as_bool() == Some(true);
                    components::status_dot(
                        ui,
                        if ready {
                            palette.success
                        } else {
                            palette.warning
                        },
                        if ready {
                            "Ready · models download on first use"
                        } else {
                            "AnimeScripter setup required"
                        },
                    );
                    let installing = self.install["status"].as_str() == Some("running");
                    if ui
                        .add_enabled(
                            self.connected && !installing && self.active_task().is_none(),
                            egui::Button::new(if ready {
                                "Repair AnimeScripter"
                            } else {
                                "Install AnimeScripter"
                            }),
                        )
                        .clicked()
                    {
                        self.post("/api/animescripter/install", json!({}));
                    }
                }
            });
            ui.label(RichText::new(if self.render_engine == "animescripter" {
                "Anime-focused processing with its own RIFE, upscaling and restoration methods. First use may download selected AI weights."
            } else {
                "Use the built-in RVE pipeline and the models from the Models page."
            }).size(12.0).color(palette.secondary));
            if self.render_engine == "animescripter"
                && !self.anime["detail"].as_str().unwrap_or_default().is_empty()
            {
                ui.label(
                    RichText::new(self.anime["detail"].as_str().unwrap_or_default())
                        .size(11.0)
                        .color(palette.muted),
                );
            }
        });
        if engine_changed {
            self.save_render_defaults();
        }
        if self.render_engine == "animescripter" {
            ui.add_space(theme::space::MD);
            components::card(ui, "AnimeScripter pipeline", |ui| {
                ui.label(RichText::new("Choose AnimeScripter's native methods. These selections are saved on this PC.").size(12.0).color(palette.secondary));
                let methods = self.anime_options["methods"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                let mut upscale = methods
                    .get("upscale")
                    .and_then(Value::as_str)
                    .unwrap_or("shufflecugan")
                    .to_owned();
                let mut interpolate = methods
                    .get("interpolate")
                    .and_then(Value::as_str)
                    .unwrap_or("rife4.6")
                    .to_owned();
                let mut restore = methods
                    .get("restore")
                    .and_then(Value::as_str)
                    .unwrap_or("anime1080fixer")
                    .to_owned();
                let mut changed = false;
                egui::ComboBox::from_label("Upscale method")
                    .selected_text(&upscale)
                    .show_ui(ui, |ui| {
                        for value in [
                            "shufflecugan",
                            "span",
                            "adore",
                            "fallinsoft",
                            "openproteus",
                            "aniscale2",
                        ] {
                            changed |= ui
                                .selectable_value(&mut upscale, value.to_owned(), value)
                                .changed();
                        }
                    });
                egui::ComboBox::from_label("Interpolation method")
                    .selected_text(&interpolate)
                    .show_ui(ui, |ui| {
                        for value in ["rife4.6", "rife4.26", "rife4.25", "distildrba"] {
                            changed |= ui
                                .selectable_value(&mut interpolate, value.to_owned(), value)
                                .changed();
                        }
                    });
                egui::ComboBox::from_label("Restoration method")
                    .selected_text(&restore)
                    .show_ui(ui, |ui| {
                        for value in [
                            "anime1080fixer",
                            "scunet",
                            "nafnet",
                            "deh264_real",
                            "dejpeg",
                        ] {
                            changed |= ui
                                .selectable_value(&mut restore, value.to_owned(), value)
                                .changed();
                        }
                    });
                if changed {
                    self.anime_options["methods"] =
                        json!({"upscale": upscale, "interpolate": interpolate, "restore": restore});
                    self.save_render_defaults();
                }
                ui.label(RichText::new("Weights download from the upstream AnimeScripter model sources as each method is first used.").size(11.0).color(palette.muted));
            });
        }
    }

    fn live_render_preview_card(&self, ui: &mut egui::Ui) {
        let preview_enabled = self.saved_settings["advanced"]["preview_enabled"]
            .as_bool()
            .unwrap_or(true);
        let status = self.render["status"].as_str().unwrap_or("idle");
        let active = matches!(status, "preparing" | "running" | "paused");
        let completed_with_preview = status == "completed" && self.render_preview.is_some();
        if !preview_enabled || (!active && !completed_with_preview) {
            return;
        }

        let palette = theme::palette(!ui.visuals().dark_mode, self.prefs.accent);
        components::card(ui, "Live render preview", |ui| {
            ui.label(
                RichText::new(if completed_with_preview {
                    "Last processed frame"
                } else {
                    "A frame from the active render · updates as processing continues"
                })
                .size(12.0)
                .color(palette.secondary),
            );
            ui.add_space(theme::space::SM);
            let width = ui.available_width().max(1.0);
            let size = Vec2::new(width, (width * 9.0 / 16.0).clamp(150.0, 360.0));
            if let Some(texture) = self.render_preview.as_ref() {
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter().rect_filled(rect, 12.0, Color32::BLACK);
                ui.place(
                    rect,
                    egui::Image::new(texture)
                        .fit_to_exact_size(size)
                        .maintain_aspect_ratio(true)
                        .corner_radius(12.0),
                );
            } else {
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter().rect_filled(rect, 12.0, palette.elevated);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Waiting for the first processed frame…",
                    egui::FontId::proportional(13.0),
                    palette.secondary,
                );
            }
        });
        ui.add_space(theme::space::MD);
    }

    fn render_summary(&self, ui: &mut egui::Ui) {
        let status = self.render["status"].as_str().unwrap_or("idle");
        let frame = self.render["current_frame"].as_u64().unwrap_or(0);
        let fps = self.render["gen_fps"].as_f64();
        let eta = self.render["eta"].as_str().unwrap_or("--");
        let light = !ui.visuals().dark_mode;
        let palette = theme::palette(light, self.prefs.accent);
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

    fn model_library_page(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.prefs.accent);
        if self.library.is_null() && !self.library_loading {
            self.library_loading = true;
            self.get("/api/library/models");
        }
        let mut search_requested = false;
        let mut refresh_requested = false;
        components::card(ui, "Explore the model community", |ui| {
            ui.horizontal(|ui| {
                ui.add(icons::styled("sparkles", palette.accent, 24.0));
                ui.vertical(|ui| {
                    ui.label(RichText::new("Find the right look for your footage").strong().size(19.0));
                    ui.label(RichText::new("Browse community models, inspect examples, and compare their formats before installing.").size(12.0).color(palette.secondary));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.hyperlink_to("OpenModelDB ↗", "https://openmodeldb.info");
                });
            });
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                ui.add(icons::styled("search", palette.muted, 18.0));
                let response = ui.add_sized(
                    Vec2::new(ui.available_width().min(520.0), 40.0),
                    egui::TextEdit::singleline(&mut self.library_search)
                        .hint_text("Try ‘anime’, ‘realistic’, ‘denoise’ or a model name"),
                );
                let submit =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                egui::ComboBox::from_id_salt("library-category")
                    .selected_text(match self.library_category.as_str() {
                        "anime" => "Anime",
                        "realistic" => "Realistic",
                        "restoration" => "Restoration",
                        "denoise" => "Denoise",
                        "compression" => "Compression",
                        _ => "All purposes",
                    })
                    .show_ui(ui, |ui| {
                        for (key, label) in [
                            ("all", "All purposes"),
                            ("anime", "Anime"),
                            ("realistic", "Realistic"),
                            ("restoration", "Restoration"),
                            ("denoise", "Denoise"),
                            ("compression", "Compression"),
                        ] {
                            ui.selectable_value(&mut self.library_category, key.to_owned(), label);
                        }
                    });
                if ui
                    .add_enabled(!self.library_loading, egui::Button::new("Search models"))
                    .clicked()
                    || submit
                {
                    search_requested = true;
                }
                if ui
                    .add_enabled(!self.library_loading, egui::Button::new("Refresh"))
                    .clicked()
                {
                    refresh_requested = true;
                }
                if self.library_loading {
                    ui.spinner();
                }
            });
            ui.add_space(theme::space::SM);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} community models",
                        self.library["count"].as_u64().unwrap_or(0)
                    ))
                    .strong()
                    .color(palette.text),
                );
                ui.label(
                    RichText::new(format!(
                        "{} matching this search",
                        self.library["results"]
                            .as_array()
                            .map(Vec::len)
                            .unwrap_or(0)
                    ))
                    .size(12.0)
                    .color(palette.secondary),
                );
                ui.label(
                    RichText::new("Preview images · architecture · scale · license")
                        .size(11.0)
                        .color(palette.muted),
                );
                if !self.library["error"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty()
                {
                    ui.colored_label(
                        palette.warning,
                        self.library["error"].as_str().unwrap_or_default(),
                    );
                }
            });
        });
        if search_requested || refresh_requested {
            self.library_loading = true;
            self.library_visible_count = 12;
            self.library_image_failed.clear();
            let endpoint = format!(
                "/api/library/models?q={}&category={}{}",
                encoded_query_value(&self.library_search),
                encoded_query_value(&self.library_category),
                if refresh_requested { "&refresh=1" } else { "" },
            );
            self.get(&endpoint);
        }

        ui.add_space(theme::space::MD);
        let total_matches = self.library["results"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        let shown_count = total_matches.min(self.library_visible_count);
        let results = self.library["results"]
            .as_array()
            .map(|items| items.iter().take(shown_count).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        components::card(ui, "Community models", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("Showing {shown_count} of {total_matches}")).strong(),
                );
                ui.label(
                    RichText::new("Select a preview to inspect the model examples on OpenModelDB.")
                        .size(11.0)
                        .color(palette.secondary),
                );
            });
            ui.add_space(theme::space::SM);
            if results.is_empty() {
                ui.label(if self.library_loading {
                    "Loading model catalogue…"
                } else {
                    "No models match that search. Try another name, purpose, or architecture."
                });
            } else {
                egui::ScrollArea::vertical()
                    .max_height(640.0)
                    .show(ui, |ui| {
                        let columns = if ui.available_width() >= 940.0 { 2 } else { 1 };
                        for row in results.chunks(columns) {
                            ui.columns(columns, |columns_ui| {
                                for (column, model) in row.iter().enumerate() {
                                    self.community_model_card(
                                        &mut columns_ui[column],
                                        model,
                                        palette,
                                    );
                                }
                            });
                            ui.add_space(theme::space::SM);
                        }
                        if shown_count < total_matches && ui.button("Load more models").clicked() {
                            self.library_visible_count =
                                (self.library_visible_count + 12).min(total_matches);
                        }
                    });
            }
        });
        if self.download["active"].as_bool() == Some(true) {
            ui.add_space(theme::space::SM);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    self.download["detail"]
                        .as_str()
                        .unwrap_or("Downloading community model…"),
                );
                if ui.button("Cancel download").clicked() {
                    self.post("/api/actions/cancel", json!({}));
                }
            });
        }
    }

    fn community_model_card(&mut self, ui: &mut egui::Ui, model: &Value, palette: theme::Palette) {
        let model_id = model["id"].as_str().unwrap_or_default();
        let name = model["name"].as_str().unwrap_or("Unnamed model");
        let url = model["url"].as_str().unwrap_or("https://openmodeldb.info");
        let architecture = model["architecture"]
            .as_str()
            .unwrap_or("Unknown architecture");
        let scale = model["scale"].as_i64().unwrap_or(0);
        let license = model["license"].as_str().unwrap_or("License not listed");
        let preview = model["images"].as_array().and_then(|images| images.first());
        let install_resource = model["resources"].as_array().and_then(|items| {
            items
                .iter()
                .find(|resource| resource["installable"].as_bool() == Some(true))
        });
        let area =
            egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 150.0));
        if preview.is_some()
            && ui.is_rect_visible(area)
            && !model_id.is_empty()
            && !self.library_images.contains_key(model_id)
            && !self.library_image_loading.contains(model_id)
            && !self.library_image_failed.contains(model_id)
        {
            self.library_image_loading.insert(model_id.to_owned());
            self.get(&format!(
                "/api/library/thumbnail?id={}",
                encoded_query_value(model_id)
            ));
        }

        egui::Frame::new()
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(14.0)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let preview_width = (ui.available_width() * 0.40).clamp(150.0, 220.0);
                    let preview_size = Vec2::new(preview_width, preview_width * 0.66);
                    egui::Frame::new()
                        .fill(palette.elevated)
                        .corner_radius(10.0)
                        .stroke(Stroke::new(1.0, palette.border))
                        .show(ui, |ui| {
                            if let Some(texture) = self.library_images.get(model_id) {
                                ui.add(
                                    egui::Image::new((texture.id(), preview_size))
                                        .fit_to_exact_size(preview_size)
                                        .corner_radius(9.0),
                                );
                            } else {
                                let (rect, _) =
                                    ui.allocate_exact_size(preview_size, egui::Sense::hover());
                                let painter = ui.painter();
                                painter.rect_filled(rect, 9.0, palette.elevated);
                                painter.circle_filled(
                                    rect.right_top() + Vec2::new(-34.0, 34.0),
                                    16.0,
                                    palette.accent.gamma_multiply(0.75),
                                );
                                painter.add(egui::Shape::convex_polygon(
                                    vec![
                                        rect.left_bottom() + Vec2::new(0.0, -7.0),
                                        rect.center() + Vec2::new(0.0, 8.0),
                                        rect.right_bottom() + Vec2::new(0.0, -7.0),
                                    ],
                                    palette.accent.gamma_multiply(0.28),
                                    Stroke::NONE,
                                ));
                                painter.text(
                                    rect.left_top() + Vec2::new(12.0, 12.0),
                                    egui::Align2::LEFT_TOP,
                                    if scale > 0 {
                                        format!("{scale}×")
                                    } else {
                                        "MODEL".to_owned()
                                    },
                                    egui::FontId::proportional(28.0),
                                    palette.text,
                                );
                                painter.text(
                                    rect.left_bottom() + Vec2::new(12.0, -12.0),
                                    egui::Align2::LEFT_BOTTOM,
                                    architecture.to_uppercase(),
                                    egui::FontId::proportional(10.0),
                                    palette.secondary,
                                );
                                if self.library_image_loading.contains(model_id) {
                                    painter.circle_stroke(
                                        rect.right_bottom() - Vec2::splat(17.0),
                                        7.0,
                                        Stroke::new(1.5, palette.accent),
                                    );
                                }
                            }
                        });
                    ui.add_space(theme::space::SM);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(name).strong().size(15.0).color(palette.text));
                        ui.label(
                            RichText::new(format!(
                                "by {}",
                                model["author"].as_str().unwrap_or("Community contributor")
                            ))
                            .size(11.0)
                            .color(palette.secondary),
                        );
                        ui.add_space(theme::space::XS);
                        ui.horizontal_wrapped(|ui| {
                            for label in [
                                architecture.to_owned(),
                                if scale > 0 {
                                    format!("{scale}×")
                                } else {
                                    "General".to_owned()
                                },
                                license.to_owned(),
                            ] {
                                egui::Frame::new()
                                    .fill(palette.elevated)
                                    .corner_radius(6.0)
                                    .inner_margin(egui::Margin::symmetric(7, 3))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(label)
                                                .size(10.0)
                                                .color(palette.secondary),
                                        );
                                    });
                            }
                        });
                        if let Some(description) = model["description"]
                            .as_str()
                            .filter(|text| !text.is_empty())
                        {
                            let preview_text = description.chars().take(190).collect::<String>();
                            ui.add_space(theme::space::XS);
                            ui.label(
                                RichText::new(if description.chars().count() > 190 {
                                    format!("{preview_text}…")
                                } else {
                                    preview_text
                                })
                                .size(11.0)
                                .color(palette.secondary),
                            );
                        }
                        if let Some(tags) = model["tags"].as_array() {
                            ui.horizontal_wrapped(|ui| {
                                for tag in tags.iter().take(4).filter_map(Value::as_str) {
                                    ui.label(RichText::new(tag).size(10.0).color(palette.muted));
                                }
                            });
                        }
                        ui.add_space(theme::space::XS);
                        ui.horizontal_wrapped(|ui| {
                            ui.hyperlink_to("View examples ↗", url);
                            if let Some(resource) = install_resource {
                                let bytes = resource["size"].as_u64().map(format_bytes);
                                let install_label = bytes.map_or_else(
                                    || "Install .pth".to_owned(),
                                    |size| format!("Install · {size}"),
                                );
                                if ui
                                    .add_enabled(
                                        !self.download["active"].as_bool().unwrap_or(false),
                                        egui::Button::new(install_label),
                                    )
                                    .clicked()
                                {
                                    self.post("/api/library/download", json!({"id": model_id}));
                                }
                            }
                        });
                    });
                });
            });
        ui.add_space(theme::space::XS);
    }
}

impl eframe::App for NativeApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        // Fully clear lets the Windows material through; otherwise stay solid.
        if self.material_active {
            [0.0; 4]
        } else if visuals.dark_mode {
            [0.043, 0.051, 0.075, 1.0]
        } else {
            [0.945, 0.953, 0.973, 1.0]
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let Ok(mut slot) = self.api.engine.lock() else {
            return;
        };
        let Some(engine) = slot.as_mut() else {
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
        self.process_tray_commands(&ctx);
        if ctx.input(|input| input.viewport().close_requested())
            && self.active_task().is_some()
            && !self.update_install_started
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_confirmation = true;
        }
        self.consume_events(&ctx);
        self.check_task_toast();
        if self.update["status"] == "downloaded"
            && !self.update_install_started
            && self.update["installer_path"].as_str() != self.failed_update.as_deref()
            && self.render["status"] != "running"
        {
            self.install_downloaded_update(&ctx);
        }
        self.receive_dropped_files(&ctx);
        let shortcut = ctx.input(|input| {
            if input.modifiers.ctrl {
                [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                    egui::Key::Num6,
                    egui::Key::Num7,
                    egui::Key::Num8,
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
        self.drive_queue();
        self.track_jobs(&ctx);
        self.draw_shell(root_ui);
    }
}

impl NativeApp {
    fn receive_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect::<Vec<_>>()
        });
        if dropped.is_empty() {
            return;
        }
        let mut videos = dropped
            .iter()
            .filter(|path| prefs::is_video_path(path))
            .map(|path| path.to_string_lossy().into_owned());
        let Some(first) = videos.next() else {
            self.show_toast(
                "Not a video",
                "Drop video files such as MP4, MKV, MOV or WebM.",
                ToastKind::Error,
            );
            return;
        };
        let rest = videos.collect::<Vec<_>>();
        if self.queue.running() {
            self.queue.add(&first);
        } else {
            self.load_source(&first);
            if !rest.is_empty() {
                self.queue.add(&first);
            }
        }
        for path in &rest {
            self.queue.add(path);
        }
        self.page = PROCESS_PAGE;
    }
}

/// The engine answers with a resolved path, which may differ only in spelling.
fn same_file(left: &str, right: &str) -> bool {
    let tidy = |path: &str| path.trim().replace('/', "\\").to_lowercase();
    tidy(left) == tidy(right)
        || matches!(
            (
                std::fs::canonicalize(left.trim()),
                std::fs::canonicalize(right.trim())
            ),
            (Ok(a), Ok(b)) if a == b
        )
}

#[cfg(windows)]
fn window_handle(ctx: &eframe::CreationContext<'_>) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match ctx.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(windows))]
fn window_handle(_: &eframe::CreationContext<'_>) -> Option<isize> {
    None
}

fn main() -> eframe::Result<()> {
    set_windows_app_identity();
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../docs/assets/rve-logo.png"))
        .unwrap_or_default();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("REAL Video Enhancer")
        .with_inner_size([1360.0, 880.0])
        .with_min_inner_size([860.0, 620.0])
        .with_resizable(true)
        .with_transparent(true)
        .with_icon(icon);
    #[cfg(windows)]
    {
        viewport = viewport.with_decorations(false);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "REAL Video Enhancer",
        options,
        Box::new(|ctx| Ok(Box::new(NativeApp::new(ctx)))),
    )
}

#[cfg(windows)]
fn set_windows_app_identity() {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
    }

    let app_id: Vec<u16> = std::ffi::OsStr::new("B1progame.REALVideoEnhancer")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // Windows uses this stable identity to group the app with its own shortcut
    // and icon in the taskbar, instead of reusing a generic cached Python icon.
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr());
    }
}

#[cfg(not(windows))]
fn set_windows_app_identity() {}

#[cfg(test)]
mod tests {
    use super::{
        HOME_HERO_COLUMN_THRESHOLD, HOME_STATUS_THREE_COLUMN_THRESHOLD, PROCESS_PAGE,
        RUNTIME_METRICS_COLUMN_THRESHOLD, active_task_from, can_start_model_download,
        encode_query_component, home_hero_uses_two_columns, home_status_uses_three_columns,
        pending_post_after_event, process_uses_two_columns, render_preview_is_available,
        runtime_metrics_use_two_columns, settings_uses_two_columns,
    };

    #[test]
    fn preview_query_encodes_windows_paths_safely() {
        assert_eq!(
            encode_query_component(r"C:\Videos\Family trip 01.mp4"),
            "C%3A%5CVideos%5CFamily%20trip%2001.mp4"
        );
    }

    #[test]
    fn live_preview_only_polls_for_a_running_render_with_preview_enabled() {
        assert!(render_preview_is_available(&serde_json::json!({
            "status": "running",
            "preview": {"available": true}
        })));
        assert!(render_preview_is_available(&serde_json::json!({
            "status": "paused",
            "preview": {"available": true}
        })));
        assert!(!render_preview_is_available(&serde_json::json!({
            "status": "completed",
            "preview": {"available": true}
        })));
        assert!(!render_preview_is_available(&serde_json::json!({
            "status": "running",
            "preview": {"available": false}
        })));
    }

    #[test]
    fn app_starts_in_process_workspace() {
        assert_eq!(PROCESS_PAGE, 1);
    }

    #[test]
    fn settings_switch_to_two_columns_at_wide_sizes() {
        assert!(!settings_uses_two_columns(959.0));
        assert!(settings_uses_two_columns(960.0));
        assert!(settings_uses_two_columns(1440.0));
    }

    #[test]
    fn process_switches_to_one_column_when_space_is_tight() {
        assert!(!process_uses_two_columns(839.0));
        assert!(!process_uses_two_columns(879.0));
        assert!(process_uses_two_columns(880.0));
        // Keeps the two-column editing workspace usable on a 1184 px desktop.
        assert!(process_uses_two_columns(1090.0));
    }

    #[test]
    fn runtime_hardware_cards_reflow_before_values_get_cramped() {
        assert!(!runtime_metrics_use_two_columns(
            RUNTIME_METRICS_COLUMN_THRESHOLD - 1.0
        ));
        assert!(runtime_metrics_use_two_columns(
            RUNTIME_METRICS_COLUMN_THRESHOLD
        ));
    }

    #[test]
    fn home_layout_reflows_before_cards_can_clip() {
        assert!(!home_hero_uses_two_columns(
            HOME_HERO_COLUMN_THRESHOLD - 1.0
        ));
        assert!(home_hero_uses_two_columns(HOME_HERO_COLUMN_THRESHOLD));
        assert!(!home_status_uses_three_columns(
            HOME_STATUS_THREE_COLUMN_THRESHOLD - 1.0
        ));
        assert!(home_status_uses_three_columns(
            HOME_STATUS_THREE_COLUMN_THRESHOLD
        ));
        // The captured 1184×950 desktop window leaves about 860 points inside
        // the Home hero and about 908 points for the status row. Keep its
        // useful two/three-column dashboard at those measured content widths.
        assert!(home_hero_uses_two_columns(860.0));
        assert!(home_status_uses_three_columns(908.0));
    }

    #[test]
    fn model_download_requires_connected_idle_engine() {
        assert!(!can_start_model_download(false, false, false));
        assert!(!can_start_model_download(true, true, false));
        assert!(!can_start_model_download(true, false, true));
        assert!(can_start_model_download(true, false, false));
    }

    #[test]
    fn background_poll_does_not_clear_a_pending_post() {
        assert!(pending_post_after_event(true, false));
        assert!(!pending_post_after_event(true, true));
        assert!(!pending_post_after_event(false, false));
    }

    #[test]
    fn active_render_prevents_a_full_close() {
        assert_eq!(
            active_task_from("running", false, "", "", "", ""),
            Some("A video render")
        );
    }

    #[test]
    fn any_background_install_or_download_prevents_a_full_close() {
        assert_eq!(
            active_task_from("idle", true, "", "", "", ""),
            Some("A model download")
        );
        assert_eq!(
            active_task_from("idle", false, "running", "", "", ""),
            Some("Runtime setup")
        );
        assert_eq!(
            active_task_from("idle", false, "", "installing", "", ""),
            Some("Model storage setup")
        );
        assert_eq!(
            active_task_from("idle", false, "", "", "downloading", ""),
            Some("An app update")
        );
        assert_eq!(
            active_task_from("idle", false, "", "", "installing", ""),
            Some("An app update")
        );
    }

    #[test]
    fn idle_app_can_close_normally() {
        assert_eq!(
            active_task_from("completed", false, "", "", "ready", ""),
            None
        );
    }
}
