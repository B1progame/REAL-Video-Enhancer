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
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(windows)]
use tray_icon::{
    Icon as TrayIconImage, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
mod ui;
use ui::theme::{accent_for_theme, border_color, secondary_text_color};
use ui::{components, icons, theme};

static API_PORT: AtomicU16 = AtomicU16::new(51837);
const APP_VERSION: &str = match option_env!("RVE_APP_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
const PAGES: [(&str, &str); 8] = [
    ("Home", "home"),
    ("Process", "process"),
    ("Downloads", "download"),
    ("Model library", "library"),
    ("Benchmarks", "benchmark"),
    ("Runtime", "runtime"),
    ("Python", "python"),
    ("Settings", "settings"),
];
const PROCESS_PAGE: usize = 1;
const BENCHMARK_PAGE: usize = 4;
const RUNTIME_PAGE: usize = 5;
const SETTINGS_PAGE: usize = 7;
const PROCESS_COLUMN_THRESHOLD: f32 = 1040.0;
const RUNTIME_METRICS_COLUMN_THRESHOLD: f32 = 760.0;
const HOME_HERO_COLUMN_THRESHOLD: f32 = 780.0;
const HOME_STATUS_THREE_COLUMN_THRESHOLD: f32 = 940.0;

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

#[derive(Clone)]
enum ApiCommand {
    Get(String),
    Post(String, Value),
}

struct ApiEvent {
    endpoint: String,
    value: Result<Value, String>,
    image: Option<Result<Vec<u8>, String>>,
    is_post: bool,
}

struct ApiBridge {
    send: Sender<ApiCommand>,
    receive: Receiver<ApiEvent>,
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

fn start_api_worker(ctx: egui::Context) -> ApiBridge {
    let (command_send, command_receive) = mpsc::channel::<ApiCommand>();
    let (event_send, event_receive) = mpsc::channel::<ApiEvent>();
    thread::Builder::new()
        .name("rve-api".to_owned())
        .spawn(move || {
            let client = agent();
            let mut next_refresh = Instant::now();
            let mut render_active = false;
            let mut connected = false;
            loop {
                match command_receive.recv_timeout(Duration::from_millis(120)) {
                    Ok(ApiCommand::Get(endpoint)) => {
                        let image = if endpoint.starts_with("/api/video/first_frame?")
                            || endpoint == "/api/render/preview_frame"
                            || endpoint.starts_with("/api/library/thumbnail?id=")
                        {
                            Some(api_get_image(&client, &endpoint))
                        } else {
                            None
                        };
                        let value = if image.is_some() {
                            image
                                .as_ref()
                                .and_then(|result| match result {
                                    Ok(_) => None,
                                    Err(error) => Some(Err(error.clone())),
                                })
                                .unwrap_or(Ok(Value::Null))
                        } else {
                            api_get(&client, &endpoint)
                        };
                        if endpoint == "/api/bootstrap" && value.is_ok() {
                            connected = true;
                        }
                        let _ = event_send.send(ApiEvent {
                            endpoint,
                            value,
                            image,
                            is_post: false,
                        });
                        ctx.request_repaint();
                    }
                    Ok(ApiCommand::Post(endpoint, payload)) => {
                        let value = api_post(&client, &endpoint, &payload);
                        let _ = event_send.send(ApiEvent {
                            endpoint,
                            value,
                            image: None,
                            is_post: true,
                        });
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
                            "/api/benchmarks/status",
                            "/api/app/update/status",
                        ]
                    } else {
                        &["/api/bootstrap"]
                    };
                    for endpoint in endpoints {
                        let value = api_get(&client, endpoint);
                        let should_fetch_render_preview = if *endpoint == "/api/render/status" {
                            render_active = value.as_ref().is_ok_and(|status| {
                                matches!(
                                    status["status"].as_str(),
                                    Some("preparing" | "running" | "paused")
                                )
                            });
                            value.as_ref().is_ok_and(render_preview_is_available)
                        } else {
                            false
                        };
                        if *endpoint == "/api/bootstrap" && value.is_ok() {
                            connected = true;
                        } else if *endpoint != "/api/bootstrap"
                            && let Err(error) = &value
                        {
                            let _ = event_send.send(ApiEvent {
                                endpoint: "/api/connection".to_owned(),
                                value: Err(error.clone()),
                                image: None,
                                is_post: false,
                            });
                            connected = false;
                            ctx.request_repaint();
                            break;
                        }
                        let _ = event_send.send(ApiEvent {
                            endpoint: (*endpoint).to_owned(),
                            value,
                            image: None,
                            is_post: false,
                        });
                        ctx.request_repaint();
                        if should_fetch_render_preview
                            && let Ok(bytes) = api_get_image(&client, "/api/render/preview_frame")
                            && !bytes.is_empty()
                        {
                            let _ = event_send.send(ApiEvent {
                                endpoint: "/api/render/preview_frame".to_owned(),
                                value: Ok(Value::Null),
                                image: Some(Ok(bytes)),
                                is_post: false,
                            });
                            ctx.request_repaint();
                        }
                    }
                    next_refresh = Instant::now()
                        + Duration::from_millis(if render_active { 800 } else { 1500 });
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
    accent: Color32,
    appearance: String,
    reduce_motion: bool,
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
    model_search: String,
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
    launch_time: Instant,
}

impl NativeApp {
    fn new(ctx: &eframe::CreationContext<'_>) -> Self {
        #[cfg(windows)]
        let (tray_icon, tray_events, tray_error) = create_tray(&ctx.egui_ctx);
        #[cfg(not(windows))]
        let (tray_events, tray_error) = create_tray(&ctx.egui_ctx);
        let accent = saved_accent();
        let appearance = saved_appearance();
        let reduce_motion = saved_reduce_motion();
        let model_storage = saved_model_storage();
        let first_setup = !model_storage_configured();
        let (engine, engine_error) = match start_engine(&model_storage) {
            Ok(engine) => (engine, None),
            Err(error) => (None, Some(error)),
        };
        egui_extras::install_image_loaders(&ctx.egui_ctx);
        apply_theme(&ctx.egui_ctx, accent, &appearance, reduce_motion);
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
            accent,
            appearance,
            reduce_motion,
            connected: false,
            engine_error,
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
            model_search: String::new(),
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

    fn post(&mut self, endpoint: &str, payload: Value) {
        if self.busy {
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

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = self.toast.as_ref() else {
            return;
        };
        if Instant::now() >= toast.expires_at {
            self.toast = None;
            return;
        }
        let palette = theme::palette(self.appearance == "light", self.accent);
        let accent = match toast.kind {
            ToastKind::Info => palette.accent,
            ToastKind::Success => palette.success,
            ToastKind::Error => palette.error,
        };
        egui::Area::new(egui::Id::unique("rve-operation-toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-22.0, 54.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette.elevated)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(12.0)
                    .inner_margin(egui::Margin::symmetric(15, 12))
                    .shadow(egui::Shadow {
                        offset: [0, 5],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(90),
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(390.0);
                        ui.horizontal_top(|ui| {
                            ui.add_space(2.0);
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot_rect.center(), 4.0, accent);
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&toast.title).strong().color(palette.text));
                                ui.label(
                                    RichText::new(&toast.detail)
                                        .size(12.0)
                                        .color(palette.secondary),
                                );
                            });
                        });
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(150));
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
                self.update["status"] = json!("error");
                self.update["detail"] =
                    json!(format!("Could not start the downloaded update: {error}"));
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

    fn hide_to_tray(&self, ctx: &egui::Context) {
        #[cfg(windows)]
        if let Some(icon) = self._tray_icon.as_ref() {
            let activity = self.active_task().unwrap_or("Work");
            let _ = icon.set_tooltip(Some(format!(
                "REAL Video Enhancer · {activity} in background"
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        #[cfg(not(windows))]
        let _ = ctx;
    }

    fn process_tray_commands(&mut self, ctx: &egui::Context) {
        while let Ok(command) = self.tray_events.try_recv() {
            match command {
                TrayCommand::Open => {
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
                if let Some(model_id) = event.endpoint.strip_prefix("/api/library/thumbnail?id=") {
                    let model_id = model_id.to_owned();
                    self.library_image_loading.remove(&model_id);
                    match image {
                        Ok(bytes) => match image::load_from_memory(&bytes) {
                            Ok(decoded) => {
                                let thumbnail = decoded.thumbnail(384, 256);
                                let rgba = thumbnail.to_rgba8();
                                let size = [rgba.width() as usize, rgba.height() as usize];
                                let color_image =
                                    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                if self.library_images.len() >= 48
                                    && let Some(expired) = self.library_image_order.pop_front()
                                {
                                    self.library_images.remove(&expired);
                                    self.library_image_failed.insert(expired);
                                }
                                self.library_images.insert(
                                    model_id.clone(),
                                    ctx.load_texture(
                                        format!("rve-model-preview-{model_id}"),
                                        color_image,
                                        egui::TextureOptions::LINEAR,
                                    ),
                                );
                                self.library_image_order.push_back(model_id.clone());
                            }
                            Err(_) => {
                                self.library_image_failed.insert(model_id);
                            }
                        },
                        Err(_) => {
                            self.library_image_failed.insert(model_id);
                        }
                    }
                    ctx.request_repaint();
                    continue;
                }
                if event.endpoint == "/api/render/preview_frame" {
                    if let Ok(bytes) = image
                        && let Ok(decoded) = image::load_from_memory(&bytes)
                    {
                        let rgba = decoded.to_rgba8();
                        let size = [rgba.width() as usize, rgba.height() as usize];
                        let color_image =
                            egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                        self.render_preview = Some(ctx.load_texture(
                            "rve-live-render-preview",
                            color_image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    ctx.request_repaint();
                    continue;
                }
                if self.source_preview_path == self.input_path.trim() {
                    self.source_preview_loading = false;
                    match image {
                        Ok(bytes) => match image::load_from_memory(&bytes) {
                            Ok(decoded) => {
                                let rgba = decoded.to_rgba8();
                                let size = [rgba.width() as usize, rgba.height() as usize];
                                let color_image =
                                    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                                self.source_preview = Some(ctx.load_texture(
                                    "rve-source-preview",
                                    color_image,
                                    egui::TextureOptions::LINEAR,
                                ));
                                self.source_preview_error = None;
                            }
                            Err(error) => {
                                self.source_preview_error =
                                    Some(format!("Could not decode the video preview: {error}"));
                            }
                        },
                        Err(error) => self.source_preview_error = Some(error),
                    }
                }
                ctx.request_repaint();
                continue;
            }
            self.busy = pending_post_after_event(self.busy, event.is_post);
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
                    "/api/render/start" => {
                        self.render["status"] = json!("preparing");
                        self.render["error"] = Value::Null;
                        self.get("/api/render/status");
                    }
                    "/api/models/download_status" => {
                        self.download = value.clone();
                        if self.library_download_refresh_pending
                            && value["active"].as_bool() != Some(true)
                        {
                            if value["status"].as_str() == Some("completed") {
                                self.get(&format!("/api/models?backend={}", self.backend));
                            }
                            self.library_download_refresh_pending = false;
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
                        self.input_info = value;
                        if self.input_info["error"].is_null() {
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
                    if event.is_post || event.endpoint == "/api/dialog/select_input" {
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

    fn source_drop_target(&mut self, ui: &mut egui::Ui, height: f32) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let available = ui.available_width().max(1.0);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(available, height), egui::Sense::click());
        let files_hovered = ui.input(|input| !input.raw.hovered_files.is_empty());
        let highlighted = response.hovered() || files_hovered;
        let fill = if highlighted {
            palette
                .accent
                .gamma_multiply(if ui.visuals().dark_mode { 0.12 } else { 0.08 })
        } else {
            palette.surface
        };
        ui.painter().rect_filled(rect, 12.0, fill);
        ui.painter().rect_stroke(
            rect,
            14.0,
            Stroke::new(
                if highlighted { 1.6 } else { 1.0 },
                if highlighted {
                    palette.accent
                } else {
                    palette.border
                },
            ),
            egui::StrokeKind::Inside,
        );
        let title = if files_hovered {
            "Release to add your video"
        } else if self.input_path.is_empty() {
            "Choose a video"
        } else {
            "Replace source video"
        };
        let icon_rect =
            egui::Rect::from_center_size(rect.center() - Vec2::new(0.0, 45.0), Vec2::splat(36.0));
        ui.place(icon_rect, icons::styled("video", palette.accent, 30.0));
        ui.painter().text(
            rect.center() + Vec2::new(0.0, 4.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(15.0),
            palette.text,
        );
        ui.painter().text(
            rect.center() + Vec2::new(0.0, 28.0),
            egui::Align2::CENTER_CENTER,
            "Click to browse, or drag a video into this window",
            egui::FontId::proportional(12.0),
            palette.secondary,
        );
        let response = response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to open File Explorer and choose a video");
        if response.clicked() {
            self.get("/api/dialog/select_input");
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                14.0,
                Stroke::new(1.5, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    }

    fn start_render(&mut self) {
        let saved_settings = self.render_defaults_snapshot();
        self.render_preview = None;
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

    fn page_title(&self) -> &'static str {
        PAGES.get(self.page).map(|page| page.0).unwrap_or("Home")
    }

    fn page_description(&self) -> &'static str {
        match self.page {
            0 => "Your workspace for cleaner, sharper video.",
            1 => "Choose a source, tune enhancements, and render.",
            2 => "Find and install the models you need.",
            3 => "Search the wider model community and check compatibility.",
            4 => "Compare installed models on your hardware, at real input resolutions.",
            5 => "Set up the inference engines and AnimeScripter.",
            6 => "Inspect the Python engine and its environment.",
            7 => "Personalize appearance, storage, and app updates.",
            _ => "Your workspace for cleaner, sharper video.",
        }
    }

    fn header(&self, ui: &mut egui::Ui) {
        let light = !ui.visuals().dark_mode;
        let palette = theme::palette(light, self.accent);
        ui.horizontal_wrapped(|ui| {
            let icon_frame = egui::Frame::new()
                .fill(palette.elevated)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(14)
                .inner_margin(egui::Margin::same(12));
            icon_frame.show(ui, |ui| {
                ui.add(icons::styled(
                    PAGES.get(self.page).map(|page| page.1).unwrap_or("home"),
                    palette.accent,
                    26.0,
                ));
            });
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(self.page_title())
                        .size(theme::type_scale::PAGE)
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
                components::status_badge(ui, status, color);
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

    #[cfg(windows)]
    fn titlebar_button(ui: &mut egui::Ui, glyph: &str, close: bool) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(46.0, 42.0), egui::Sense::click());
        let center = rect.center();
        let stroke = Stroke::new(1.15, ui.visuals().text_color().gamma_multiply(0.78));
        if response.hovered() {
            let fill = if close {
                Color32::from_rgb(196, 54, 66)
            } else {
                ui.visuals().widgets.hovered.bg_fill
            };
            ui.painter().rect_filled(rect, 0.0, fill);
        }
        match glyph {
            "minimize" => {
                ui.painter().line_segment(
                    [center + Vec2::new(-5.0, 3.0), center + Vec2::new(5.0, 3.0)],
                    stroke,
                );
            }
            "maximize" => {
                ui.painter().rect_stroke(
                    egui::Rect::from_center_size(center, Vec2::splat(10.0)),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            "restore" => {
                let rear =
                    egui::Rect::from_center_size(center + Vec2::new(1.5, -1.5), Vec2::splat(9.0));
                let front =
                    egui::Rect::from_center_size(center + Vec2::new(-1.5, 1.5), Vec2::splat(9.0));
                ui.painter()
                    .rect_filled(front, 0.0, ui.visuals().panel_fill);
                ui.painter()
                    .rect_stroke(rear, 0.0, stroke, egui::StrokeKind::Inside);
                ui.painter()
                    .rect_stroke(front, 0.0, stroke, egui::StrokeKind::Inside);
            }
            "close" => {
                ui.painter().line_segment(
                    [center + Vec2::new(-4.5, -4.5), center + Vec2::new(4.5, 4.5)],
                    stroke,
                );
                ui.painter().line_segment(
                    [center + Vec2::new(4.5, -4.5), center + Vec2::new(-4.5, 4.5)],
                    stroke,
                );
            }
            _ => {}
        }
        response
    }

    #[cfg(windows)]
    fn custom_titlebar(&self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let light = self.appearance == "light";
        let palette = theme::palette(light, self.accent);
        egui::Panel::top("custom-window-titlebar")
            .exact_size(42.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(palette.canvas)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ui, |bar| {
                bar.spacing_mut().item_spacing.x = 0.0;
                bar.horizontal(|bar| {
                    bar.add_space(12.0);
                    egui::Frame::new()
                        .fill(palette.elevated)
                        .stroke(Stroke::new(1.0, palette.border))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::same(2))
                        .show(bar, |icon_ui| Self::brand_mark(icon_ui, 24.0));
                    bar.add_space(9.0);
                    bar.label(
                        RichText::new("REAL Video Enhancer")
                            .size(13.0)
                            .strong()
                            .color(palette.text),
                    );

                    let drag_width = (bar.available_width() - 138.0).max(0.0);
                    let (_, drag) = bar.allocate_exact_size(
                        Vec2::new(drag_width, 42.0),
                        egui::Sense::click_and_drag(),
                    );
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if drag.double_clicked() {
                        let maximized =
                            ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    let _ = drag.on_hover_cursor(egui::CursorIcon::Move);

                    if Self::titlebar_button(bar, "minimize", false).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                    if Self::titlebar_button(
                        bar,
                        if maximized { "restore" } else { "maximize" },
                        false,
                    )
                    .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    if Self::titlebar_button(bar, "close", true).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                let rect = bar.max_rect();
                bar.painter().hline(
                    rect.x_range(),
                    rect.bottom() - 0.5,
                    Stroke::new(1.0, palette.border),
                );
            });
    }

    fn home_hero_intro(&mut self, ui: &mut egui::Ui, palette: theme::Palette) {
        ui.label(
            RichText::new("REAL VIDEO ENHANCER")
                .size(11.0)
                .strong()
                .color(palette.accent),
        );
        ui.add_space(theme::space::SM);
        ui.label(
            RichText::new("Make more of\nevery frame.")
                .size(34.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(theme::space::SM);
        ui.label(
            RichText::new("Upscale, smooth motion, and restore detail in a clear, local workflow.")
                .size(15.0)
                .color(palette.secondary),
        );
        ui.add_space(theme::space::LG);
        if components::primary_button(
            ui,
            if self.input_path.is_empty() {
                "Choose a video"
            } else {
                "Continue editing"
            },
        )
        .clicked()
        {
            self.page = PROCESS_PAGE;
            if self.input_path.is_empty() {
                self.get("/api/dialog/select_input");
            }
        }
        ui.add_space(theme::space::SM);
        ui.label(
            RichText::new("Your footage stays on your device.")
                .size(11.0)
                .color(palette.muted),
        );
    }

    fn home_video_preview(
        &mut self,
        ui: &mut egui::Ui,
        palette: theme::Palette,
        model_count: usize,
    ) {
        let width = ui.available_width().max(1.0);
        let size = Vec2::new(width, (width * 9.0 / 16.0).clamp(190.0, 310.0));
        let preview = self
            .source_preview
            .as_ref()
            .filter(|_| self.source_preview_path == self.input_path.trim())
            .cloned();
        if self.input_path.is_empty() {
            self.source_drop_target(ui, size.y);
        } else {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::click());
            ui.painter().rect_filled(rect, 12.0, palette.elevated);
            if let Some(texture) = preview {
                ui.place(
                    rect,
                    egui::Image::new(&texture)
                        .fit_to_exact_size(size)
                        .maintain_aspect_ratio(true)
                        .corner_radius(12.0),
                );
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Preparing your video preview…",
                    egui::FontId::proportional(13.0),
                    palette.secondary,
                );
            }
        }
        ui.add_space(theme::space::SM);
        let file_name = Path::new(&self.input_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("No source selected");
        let count = if !self.connected {
            "Catalog unavailable".to_owned()
        } else if model_count == 1 {
            "1 model option".to_owned()
        } else {
            format!("{model_count} model options")
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(file_name).strong().color(palette.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(count).size(11.0).color(palette.muted));
            });
        });
    }

    fn home_status_cards(
        &self,
        ui: &mut egui::Ui,
        palette: theme::Palette,
        backends: &[String],
        selected_backend_ready: bool,
        model_count: usize,
    ) {
        let engine = |ui: &mut egui::Ui| {
            Self::card(ui, "Engine", |ui| {
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
        };
        let backend = |ui: &mut egui::Ui| {
            Self::card(ui, "Inference backend", |ui| {
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
        };
        let storage = |ui: &mut egui::Ui| {
            Self::card(ui, "Model storage", |ui| {
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
        };

        let width = ui.available_width();
        if home_status_uses_three_columns(width) {
            ui.columns(3, |columns| {
                engine(&mut columns[0]);
                backend(&mut columns[1]);
                storage(&mut columns[2]);
            });
        } else if width >= 640.0 {
            ui.columns(2, |columns| {
                engine(&mut columns[0]);
                backend(&mut columns[1]);
            });
            ui.add_space(theme::space::MD);
            storage(ui);
        } else {
            engine(ui);
            ui.add_space(theme::space::MD);
            backend(ui);
            ui.add_space(theme::space::MD);
            storage(ui);
        }
    }

    fn home_page(&mut self, ui: &mut egui::Ui) {
        let light = !ui.visuals().dark_mode;
        let palette = theme::palette(light, self.accent);
        let backends = self.bootstrap["system"]["available_backends"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let selected_backend_ready = backend_is_installed(
            &self.bootstrap["system"]["available_backends"],
            &self.backend,
        );
        let model_count = self.models.as_object().map_or(0, |categories| {
            categories
                .values()
                .filter_map(Value::as_array)
                .map(Vec::len)
                .sum::<usize>()
        });
        let disk_space = self.bootstrap["system"]["available_disk_gb"].as_f64();

        egui::Frame::new()
            .fill(palette.surface)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(theme::radius::CARD)
            .inner_margin(egui::Margin::same(24))
            .show(ui, |ui| {
                if home_hero_uses_two_columns(ui.available_width()) {
                    ui.columns(2, |columns| {
                        self.home_hero_intro(&mut columns[0], palette);
                        self.home_video_preview(&mut columns[1], palette, model_count);
                    });
                } else {
                    self.home_hero_intro(ui, palette);
                    ui.add_space(theme::space::LG);
                    self.home_video_preview(ui, palette, model_count);
                }
            });

        ui.add_space(theme::space::LG);
        self.home_status_cards(ui, palette, &backends, selected_backend_ready, model_count);

        ui.add_space(theme::space::LG);
        Self::card(ui, "Quick actions", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Install runtime"))
                    .clicked()
                {
                    self.start_recommended_runtime_setup();
                    self.page = RUNTIME_PAGE;
                }
                if ui.button("Manage models").clicked() {
                    self.page = 2;
                }
                if ui.button("Compare models").clicked() {
                    self.page = BENCHMARK_PAGE;
                }
                if ui.button("Settings").clicked() {
                    self.page = SETTINGS_PAGE;
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
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let compact_source =
            ui.available_width() < 760.0 || ui.ctx().content_rect().height() < 700.0;
        Self::card(ui, "Source video", |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Original footage")
                        .strong()
                        .color(palette.text),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.input_path.is_empty() && ui.button("Replace video").clicked() {
                        self.get("/api/dialog/select_input");
                    }
                    if self.source_preview_loading {
                        components::status_dot(ui, palette.accent, "Analyzing");
                    } else if !self.input_path.is_empty() && self.input_info["error"].is_null() {
                        components::status_dot(ui, palette.success, "Ready");
                    }
                });
            });
            ui.add_space(theme::space::SM);
            if !compact_source {
                self.manual_video_path(ui);
            }
            ui.add_space(theme::space::SM);
            let preview_width = ui.available_width().max(1.0);
            let preview_size = Vec2::new(
                preview_width,
                (preview_width * 9.0 / 16.0).clamp(190.0, 340.0),
            );
            if let Some(texture) = self
                .source_preview
                .as_ref()
                .filter(|_| self.source_preview_path == self.input_path.trim())
            {
                let (rect, _) = ui.allocate_exact_size(preview_size, egui::Sense::hover());
                ui.painter().rect_filled(rect, 12.0, Color32::BLACK);
                ui.place(
                    rect,
                    egui::Image::new(texture)
                        .fit_to_exact_size(preview_size)
                        .maintain_aspect_ratio(true)
                        .corner_radius(12.0),
                );
            } else if self.input_path.trim().is_empty() {
                self.source_drop_target(ui, if compact_source { 152.0 } else { 218.0 });
            } else {
                let (rect, _) = ui.allocate_exact_size(preview_size, egui::Sense::hover());
                ui.painter().rect_filled(rect, 12.0, palette.elevated);
                let message = if self.source_preview_loading {
                    "Preparing a frame from your video…"
                } else if self.input_info["error"].is_string() {
                    "This video could not be previewed"
                } else {
                    "Video preview unavailable"
                };
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    message,
                    egui::FontId::proportional(13.0),
                    palette.secondary,
                );
            }
            if !self.input_path.trim().is_empty() {
                let file_name = Path::new(self.input_path.trim())
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(self.input_path.trim());
                ui.add_space(theme::space::SM);
                ui.add(
                    egui::Label::new(
                        RichText::new(file_name)
                            .size(14.0)
                            .strong()
                            .color(palette.text),
                    )
                    .truncate(),
                )
                .on_hover_text(file_name);
                components::copyable_path(ui, self.input_path.trim(), "Source path");
            }
            if compact_source {
                ui.add_space(theme::space::SM);
                self.manual_video_path(ui);
            }
            if let Some(error) = self.input_info["error"].as_str() {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).warning,
                    format!("Video details could not be read: {error}"),
                );
            } else if let (Some(width), Some(height)) = (
                self.input_info["width"].as_u64(),
                self.input_info["height"].as_u64(),
            ) {
                let fps = self.input_info["fps"].as_f64().unwrap_or(0.0);
                let duration = self.input_info["duration_seconds"].as_f64().unwrap_or(0.0);
                let codec = self.input_info["codec"].as_str().unwrap_or("unknown codec");
                let duration_label = if duration >= 3600.0 {
                    format!(
                        "{:02}:{:02}:{:02}",
                        (duration / 3600.0) as u32,
                        (duration / 60.0) as u32 % 60,
                        duration as u32 % 60
                    )
                } else {
                    format!(
                        "{:02}:{:02}",
                        (duration / 60.0) as u32,
                        duration as u32 % 60
                    )
                };
                ui.add_space(theme::space::SM);
                ui.horizontal_wrapped(|ui| {
                    components::metadata_chip(ui, &format!("{width} × {height}"), self.accent);
                    components::metadata_chip(ui, &format!("{fps:.2} fps"), self.accent);
                    components::metadata_chip(ui, &duration_label, self.accent);
                    components::metadata_chip(ui, &codec.to_uppercase(), self.accent);
                    if let Some(bytes) = self.input_info["size_bytes"].as_u64() {
                        let size = if bytes >= 1_073_741_824 {
                            format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
                        } else {
                            format!("{:.0} MB", bytes as f64 / 1_048_576.0)
                        };
                        components::metadata_chip(ui, &size, self.accent);
                    }
                });
            }
            if let Some(error) = self.source_preview_error.as_deref() {
                ui.colored_label(palette.warning, format!("Preview: {error}"));
            }
        });
    }

    fn manual_video_path(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Enter a video path manually")
            .id_salt("manual-video-path")
            .show(ui, |ui| {
                let compact = ui.available_width() < 360.0;
                let path_response = if compact {
                    ui.add_sized(
                        Vec2::new(ui.available_width().max(1.0), 38.0),
                        egui::TextEdit::singleline(&mut self.input_path)
                            .hint_text("Paste a video file path"),
                    )
                } else {
                    ui.horizontal(|ui| {
                        let field_width = (ui.available_width() - 78.0).max(1.0);
                        let response = ui.add_sized(
                            Vec2::new(field_width, 38.0),
                            egui::TextEdit::singleline(&mut self.input_path)
                                .hint_text("Paste a video file path"),
                        );
                        if ui.button("Inspect").clicked() {
                            self.input_info = json!({"status": "inspecting"});
                            self.post(
                                "/api/video/inspect",
                                json!({"path": self.input_path.trim()}),
                            );
                        }
                        response
                    })
                    .inner
                };
                if path_response.changed() {
                    self.input_info = Value::Null;
                    self.source_preview = None;
                    self.source_preview_path.clear();
                    self.source_preview_loading = false;
                    self.source_preview_error = None;
                }
                if compact && ui.button("Inspect video").clicked() {
                    self.input_info = json!({"status": "inspecting"});
                    self.post(
                        "/api/video/inspect",
                        json!({"path": self.input_path.trim()}),
                    );
                }
            });
    }

    fn output_configuration_card(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        Self::card(ui, "Output configuration", |ui| {
            if self.output_to_mpv {
                components::status_dot(ui, palette.accent, "Send enhanced video to MPV");
            } else if ui.available_width() < 680.0 {
                ui.label(RichText::new("Save location").strong());
                ui.label(
                    RichText::new("The engine creates the enhanced file here.")
                        .size(11.0)
                        .color(palette.secondary),
                );
                ui.add_space(theme::space::XS);
                ui.add_sized(
                    Vec2::new(ui.available_width().max(1.0), 38.0),
                    egui::TextEdit::singleline(&mut self.output_folder)
                        .hint_text("Choose an output folder"),
                );
                if ui.button("Browse for output folder").clicked() {
                    self.get("/api/dialog/select_output");
                }
            } else {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Save location").strong());
                        ui.label(
                            RichText::new("The engine creates the enhanced file here.")
                                .size(11.0)
                                .color(palette.secondary),
                        );
                    });
                    ui.add_space(theme::space::MD);
                    let path_width = (ui.available_width() - 84.0).max(1.0);
                    ui.add_sized(
                        Vec2::new(path_width, 38.0),
                        egui::TextEdit::singleline(&mut self.output_folder)
                            .hint_text("Choose an output folder"),
                    );
                    if ui.button("Browse").clicked() {
                        self.get("/api/dialog/select_output");
                    }
                });
            }
        });
    }

    fn advanced_processing_card(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let installed_backends = self.bootstrap["system"]["available_backends"].clone();
        let mut changed = false;
        let mut preview_enabled = self.saved_settings["advanced"]["preview_enabled"]
            .as_bool()
            .unwrap_or(true);
        Self::card(ui, "Advanced settings", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Processing backend").color(palette.secondary));
                let selected_backend_ready =
                    backend_is_installed(&installed_backends, &self.backend);
                egui::ComboBox::from_id_salt("process-backend")
                    .selected_text(self.backend.to_uppercase())
                    .show_ui(ui, |ui| {
                        for backend in ["pytorch", "ncnn", "tensorrt"] {
                            let available = backend_is_installed(&installed_backends, backend);
                            let label = if available {
                                backend.to_uppercase()
                            } else {
                                format!("{} · Install in Runtime", backend.to_uppercase())
                            };
                            if ui
                                .add_enabled_ui(available, |ui| {
                                    ui.selectable_value(
                                        &mut self.backend,
                                        backend.to_owned(),
                                        label,
                                    )
                                })
                                .inner
                                .changed()
                            {
                                self.get(&format!("/api/models?backend={}", self.backend));
                                changed = true;
                            }
                        }
                    });
                components::status_dot(
                    ui,
                    if selected_backend_ready {
                        palette.success
                    } else {
                        palette.warning
                    },
                    if selected_backend_ready {
                        "Available"
                    } else {
                        "Setup required"
                    },
                );
                if !selected_backend_ready && ui.small_button("Runtime setup").clicked() {
                    self.page = RUNTIME_PAGE;
                }
            });

            ui.add_space(theme::space::SM);
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("process-precision")
                    .selected_text(format!("Precision · {}", self.precision))
                    .show_ui(ui, |ui| {
                        for option in ["auto", "float16", "float32"] {
                            changed |= ui
                                .selectable_value(&mut self.precision, option.to_owned(), option)
                                .changed();
                        }
                    });
                ui.label(RichText::new("Frame multiplier").color(palette.secondary));
                changed |= ui
                    .add(egui::Slider::new(&mut self.interpolate_factor, 2.0..=8.0).suffix("×"))
                    .changed();
                ui.label(RichText::new("Scale override").color(palette.secondary));
                changed |= ui
                    .add(egui::Slider::new(&mut self.upscale_scale, 0..=4).suffix("×"))
                    .changed();
            });

            ui.add_space(theme::space::SM);
            ui.horizontal_wrapped(|ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.tile_size)
                            .range(0..=4096)
                            .prefix("Tile "),
                    )
                    .on_hover_text("Use 0 for automatic tile sizing.")
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut self.overlap)
                            .range(0..=256)
                            .prefix("Overlap "),
                    )
                    .changed();
                if self.backend == "pytorch" {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.pytorch_gpu_id)
                                .range(0..=64)
                                .prefix("GPU "),
                        )
                        .changed();
                } else if self.backend == "ncnn" {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.ncnn_gpu_id)
                                .range(0..=64)
                                .prefix("GPU "),
                        )
                        .changed();
                }
            });

            ui.add_space(theme::space::XS);
            ui.horizontal_wrapped(|ui| {
                changed |= ui.checkbox(&mut self.auto_hdr, "Automatic HDR").changed();
                changed |= ui.checkbox(&mut self.uhd_mode, "UHD mode").changed();
                changed |= ui.checkbox(&mut self.auto_crop, "Detect borders").changed();
                changed |= ui.checkbox(&mut self.overwrite, "Replace output").changed();
                changed |= ui.checkbox(&mut preview_enabled, "Live preview").changed();
            });
        });
        if changed {
            if !self.saved_settings["advanced"].is_object() {
                self.saved_settings["advanced"] = json!({});
            }
            self.saved_settings["advanced"]["preview_enabled"] = json!(preview_enabled);
            self.save_render_defaults();
        }
    }

    fn enhancement_models_card(&mut self, ui: &mut egui::Ui) {
        let mut selection_changed = false;
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let has_model_options = self.models.as_object().is_some_and(|categories| {
            categories.iter().any(|(key, value)| {
                key != "model_status" && value.as_array().is_some_and(|models| !models.is_empty())
            })
        });
        Self::card(ui, "Enhancement models", |ui| {
            ui.label(
                RichText::new("Choose the improvements for this render.")
                    .size(12.0)
                    .color(palette.secondary),
            );
            if !self.connected || !has_model_options {
                ui.add_space(theme::space::XS);
                ui.label(
                    RichText::new(if !self.connected {
                        "Model choices will appear when the local engine connects."
                    } else {
                        "No models are available for this backend. Check Downloads."
                    })
                    .size(11.0)
                    .color(palette.warning),
                );
            }
            ui.add_space(theme::space::SM);
            for (key, label, detail) in [
                ("upscale", "Upscale", "Increase image resolution"),
                ("interpolate", "Interpolate", "Create smoother motion"),
                ("decompress", "Decompress", "Restore compression detail"),
                ("denoise", "Denoise", "Reduce image noise"),
                ("deblur", "Deblur", "Recover edge definition"),
            ] {
                egui::Frame::new()
                    .fill(palette.surface)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            egui::Frame::new()
                                .fill(palette.accent.gamma_multiply(if ui.visuals().dark_mode {
                                    0.14
                                } else {
                                    0.10
                                }))
                                .corner_radius(8.0)
                                .inner_margin(egui::Margin::same(7))
                                .show(ui, |ui| {
                                    ui.add(icons::styled(key, palette.accent, 19.0));
                                });
                            ui.vertical(|ui| {
                                ui.label(RichText::new(label).strong().color(palette.text));
                                ui.label(RichText::new(detail).size(11.0).color(palette.secondary));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let enabled = self.toggles.entry(key.to_owned()).or_insert(false);
                                selection_changed |= components::toggle(
                                    ui,
                                    enabled,
                                    label,
                                    self.accent,
                                    self.reduce_motion,
                                )
                                .changed();
                            });
                        });
                        let options = self.model_choices(key);
                        if self.render_engine == "rve" && !options.is_empty() {
                            ui.add_space(theme::space::XS);
                            selection_changed |= self.model_picker(ui, key);
                            let selected = self
                                .selected_models
                                .get(key)
                                .map(String::as_str)
                                .unwrap_or_default();
                            if !selected.is_empty() {
                                let installed = self.models["model_status"][key][selected]
                                    .as_bool()
                                    == Some(true);
                                let (status, color) = if installed {
                                    ("Installed", palette.success)
                                } else {
                                    ("Download required", palette.warning)
                                };
                                components::status_dot(ui, color, status);
                            }
                        }
                    });
                ui.add_space(theme::space::XS);
            }
            if self.render_engine == "animescripter" {
                ui.label(RichText::new("AnimeScripter supplies and downloads its own AI models when you run a method for the first time.").size(11.0).color(palette.muted));
            }
            ui.horizontal(|ui| {
                ui.add(icons::styled("slomo", palette.secondary, 18.0));
                ui.label(RichText::new("Slow motion output").color(palette.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let enabled = self.toggles.entry("slomo".to_owned()).or_insert(false);
                    selection_changed |= components::toggle(
                        ui,
                        enabled,
                        "Slow motion output",
                        self.accent,
                        self.reduce_motion,
                    )
                    .changed();
                });
            });
        });
        if selection_changed {
            self.save_render_defaults();
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

    fn workflow_steps(&self, ui: &mut egui::Ui) {
        let source_ready = self.input_info["width"].as_u64().is_some();
        let has_enhancement = ["upscale", "interpolate", "decompress", "denoise", "deblur"]
            .iter()
            .any(|key| self.toggles.get(*key).copied().unwrap_or(false));
        let output_ready = self.output_to_mpv || !self.output_folder.trim().is_empty();
        let states = [source_ready, has_enhancement, output_ready];
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        ui.horizontal_wrapped(|ui| {
            for (index, (label, complete)) in ["Source", "Enhance", "Output"]
                .into_iter()
                .zip(states)
                .enumerate()
            {
                let color = if complete {
                    palette.success
                } else {
                    palette.secondary
                };
                egui::Frame::new()
                    .fill(if complete {
                        palette.success.gamma_multiply(0.10)
                    } else {
                        palette.elevated
                    })
                    .corner_radius(20.0)
                    .inner_margin(egui::Margin::symmetric(12, 7))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if complete {
                                ui.add(icons::styled("check", color, 14.0));
                            } else {
                                ui.label(
                                    RichText::new((index + 1).to_string()).strong().color(color),
                                );
                            }
                            ui.label(RichText::new(label).strong().color(palette.text));
                        });
                    });
                if index < 2 {
                    ui.label(RichText::new("›").color(palette.muted));
                }
            }
        });
    }

    fn render_action_controls(
        &mut self,
        ui: &mut egui::Ui,
        active: bool,
        preparing: bool,
        paused: bool,
        status: &str,
        can_start: bool,
    ) {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if active {
                if !preparing
                    && status != "stopping"
                    && self.render["engine"].as_str() != Some("animescripter")
                {
                    if ui.button(if paused { "Resume" } else { "Pause" }).clicked() {
                        self.post("/api/render/pause", json!({"paused": !paused}));
                    }
                    if ui.button("Cancel render").clicked() {
                        self.post("/api/render/stop", json!({}));
                    }
                }
                let label = if preparing {
                    "Preparing…"
                } else if status == "stopping" {
                    "Stopping…"
                } else {
                    "Render in progress"
                };
                ui.add_enabled(
                    false,
                    egui::Button::new(label).min_size(Vec2::new(190.0, 44.0)),
                );
            } else {
                let response =
                    components::primary_button_enabled(ui, "Start render", can_start, 190.0);
                if response.clicked() {
                    self.start_render();
                }
            }
        });
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

    fn render_action_bar(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let status = self.render["status"].as_str().unwrap_or("idle").to_owned();
        let preparing = status == "preparing";
        let active = matches!(
            status.as_str(),
            "preparing" | "running" | "paused" | "stopping"
        );
        let paused = status == "paused";
        let blockers = self.render_blockers();
        let can_start = blockers.is_empty() && !active;
        let title = if preparing {
            "Preparing render"
        } else if paused {
            "Render paused"
        } else if active {
            "Rendering video"
        } else if status == "completed" {
            "Render complete"
        } else if status == "error" {
            "Render needs attention"
        } else if !blockers.is_empty() {
            "Setup required"
        } else {
            "Ready to render"
        };
        let compact = ui.available_width() < 700.0;
        if compact {
            ui.horizontal_wrapped(|ui| {
                components::status_dot(
                    ui,
                    if active {
                        palette.accent
                    } else if !blockers.is_empty() {
                        palette.warning
                    } else {
                        palette.success
                    },
                    title,
                );
                self.render_action_controls(ui, active, preparing, paused, &status, can_start);
            });
            if active {
                let frame = self.render["current_frame"].as_u64().unwrap_or(0);
                let fps = self.render["gen_fps"].as_f64();
                let eta = self.render["eta"].as_str().unwrap_or("--");
                let mut details = Vec::new();
                if frame > 0 {
                    details.push(format!("Frame {frame}"));
                }
                if let Some(fps) = fps {
                    details.push(format!("{fps:.1} fps"));
                }
                if eta != "--" && !eta.is_empty() {
                    details.push(format!("ETA {eta}"));
                }
                ui.label(
                    RichText::new(if details.is_empty() {
                        "The renderer is starting…".to_owned()
                    } else {
                        details.join("  ·  ")
                    })
                    .size(11.0)
                    .color(palette.secondary),
                );
            } else if let Some(blocker) = blockers.first() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(blocker).size(11.0).color(palette.warning));
                    self.render_blocker_action(ui, blocker);
                });
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    components::status_dot(
                        ui,
                        if active {
                            palette.accent
                        } else if !blockers.is_empty() {
                            palette.warning
                        } else {
                            palette.success
                        },
                        title,
                    );
                    if active {
                        let frame = self.render["current_frame"].as_u64().unwrap_or(0);
                        let fps = self.render["gen_fps"].as_f64();
                        let eta = self.render["eta"].as_str().unwrap_or("--");
                        let mut details = Vec::new();
                        if frame > 0 {
                            details.push(format!("Frame {frame}"));
                        }
                        if let Some(fps) = fps {
                            details.push(format!("{fps:.1} fps"));
                        }
                        if eta != "--" && !eta.is_empty() {
                            details.push(format!("ETA {eta}"));
                        }
                        ui.label(
                            RichText::new(if details.is_empty() {
                                "The renderer is starting…".to_owned()
                            } else {
                                details.join("  ·  ")
                            })
                            .size(11.0)
                            .color(palette.secondary),
                        );
                    } else if let Some(blocker) = blockers.first() {
                        ui.label(RichText::new(blocker).size(11.0).color(palette.warning));
                    } else {
                        ui.label(
                            RichText::new("Your source, enhancements, and output are set.")
                                .size(11.0)
                                .color(palette.secondary),
                        );
                    }
                });
                if !active && let Some(blocker) = blockers.first() {
                    self.render_blocker_action(ui, blocker);
                }
                self.render_action_controls(ui, active, preparing, paused, &status, can_start);
            });
        }
        if let Some(error) = self.render["error"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            ui.colored_label(palette.error, error);
        }
    }

    fn process_page(&mut self, ui: &mut egui::Ui) {
        if self.first_setup {
            let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
            egui::Frame::new()
                .fill(palette.warning.gamma_multiply(0.10))
                .stroke(Stroke::new(1.0, palette.warning.gamma_multiply(0.35)))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("First setup").strong().color(palette.text));
                        ui.label(
                            RichText::new(
                                "Choose model storage and prepare the runtime before rendering.",
                            )
                            .color(palette.secondary),
                        );
                        if ui.button("Set up now").clicked() {
                            self.page = SETTINGS_PAGE;
                        }
                    });
                });
            ui.add_space(theme::space::MD);
        }
        self.processing_engine_card(ui);
        ui.add_space(theme::space::MD);
        self.workflow_steps(ui);
        ui.add_space(theme::space::MD);

        if !process_uses_two_columns(ui.available_width()) {
            self.source_and_output_card(ui);
            ui.add_space(theme::space::MD);
            self.enhancement_models_card(ui);
            ui.add_space(theme::space::MD);
            self.output_configuration_card(ui);
            ui.add_space(theme::space::MD);
            self.advanced_processing_card(ui);
        } else {
            ui.columns(2, |columns| {
                self.source_and_output_card(&mut columns[0]);
                self.enhancement_models_card(&mut columns[1]);
            });
            ui.add_space(theme::space::MD);
            ui.columns(2, |columns| {
                self.output_configuration_card(&mut columns[0]);
                self.advanced_processing_card(&mut columns[1]);
            });
        }
        ui.add_space(theme::space::MD);
        self.live_render_preview_card(ui);
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
    }

    fn processing_engine_card(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let mut engine_changed = false;
        Self::card(ui, "Processing engine", |ui| {
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_label("Engine")
                    .selected_text(if self.render_engine == "animescripter" {
                        "AnimeScripter"
                    } else {
                        "REAL Video Enhancer"
                    })
                    .show_ui(ui, |ui| {
                        engine_changed |= ui
                            .selectable_value(
                                &mut self.render_engine,
                                "rve".to_owned(),
                                "REAL Video Enhancer",
                            )
                            .changed();
                        engine_changed |= ui
                            .selectable_value(
                                &mut self.render_engine,
                                "animescripter".to_owned(),
                                "AnimeScripter",
                            )
                            .changed();
                    });
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
                } else {
                    components::status_dot(ui, palette.success, "REAL Video Enhancer engine");
                }
            });
            ui.label(RichText::new(if self.render_engine == "animescripter" {
                "Anime-focused processing with its own RIFE, upscaling and restoration methods. First use may download selected AI weights."
            } else {
                "Use the built-in RVE pipeline and the models installed from Downloads."
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
            Self::card(ui, "AnimeScripter pipeline", |ui| {
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

        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        Self::card(ui, "Live render preview", |ui| {
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
        let busy = self.busy || self.benchmarks["status"].as_str() == Some("running");
        let connected = self.connected;
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
                    (name.replace('_', " "), name.clone(), names)
                })
                .collect::<Vec<_>>()
        });

        Self::card(ui, "Download models", |ui| {
            ui.label(RichText::new("Download only the model weights you need. They are stored in the location selected in Settings.").color(palette.secondary));
            ui.add_space(theme::space::MD);
            if !connected {
                egui::Frame::new()
                    .fill(palette.warning.gamma_multiply(0.10))
                    .stroke(Stroke::new(1.0, palette.warning.gamma_multiply(0.35)))
                    .corner_radius(theme::radius::CONTROL)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Waiting for the local engine. Model downloads unlock when it connects.")
                                .color(palette.warning),
                        );
                    });
                ui.add_space(theme::space::SM);
            }
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
                    let enabled = can_start_model_download(connected, active, busy);
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
                if active && ui.button("Cancel model download").clicked() {
                    self.post("/api/actions/cancel", json!({}));
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
            ui.add_space(theme::space::SM);
            let query = self.model_search.trim().to_lowercase();
            ui.horizontal(|ui| {
                ui.add(icons::styled("search", palette.muted, 16.0));
                ui.add_sized(
                    Vec2::new(ui.available_width().min(420.0), 38.0),
                    egui::TextEdit::singleline(&mut self.model_search)
                        .hint_text("Search model names or categories…")
                        .margin(Vec2::new(12.0, 9.0)),
                );
                let visible = catalog
                    .iter()
                    .map(|(display, _, names)| {
                        if query.is_empty() || display.to_lowercase().contains(&query) {
                            names.len()
                        } else {
                            names
                                .iter()
                                .filter(|name| name.to_lowercase().contains(&query))
                                .count()
                        }
                    })
                    .sum::<usize>();
                ui.label(
                    RichText::new(format!("{visible} shown"))
                        .size(11.0)
                        .color(palette.muted),
                );
            });
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    if catalog.is_empty() {
                        ui.label(if self.connected {
                            "No model categories are available for this backend."
                        } else {
                            "The model catalog appears when the local engine connects."
                        });
                    } else if !query.is_empty()
                        && catalog.iter().all(|(display, _, names)| {
                            !display.to_lowercase().contains(&query)
                                && names
                                    .iter()
                                    .all(|name| !name.to_lowercase().contains(&query))
                        })
                    {
                        ui.label("No models match this search.");
                    }
                    for (display, key, names) in &catalog {
                        let filtered = names
                            .iter()
                            .filter(|name| {
                                query.is_empty()
                                    || name.to_lowercase().contains(&query)
                                    || display.to_lowercase().contains(&query)
                            })
                            .collect::<Vec<_>>();
                        if filtered.is_empty() {
                            continue;
                        }
                        let installed_visible = filtered
                            .iter()
                            .filter(|name| {
                                self.models["model_status"][key][name.as_str()].as_bool()
                                    == Some(true)
                            })
                            .count();
                        egui::CollapsingHeader::new(format!(
                            "{display}  ·  {installed_visible}/{} installed",
                            filtered.len()
                        ))
                        .id_salt(("model-category", key))
                        .show(ui, |ui| {
                            for name in filtered {
                                let ready = self.models["model_status"][key][name.as_str()]
                                    .as_bool()
                                    == Some(true);
                                egui::Frame::new()
                                    .fill(palette.elevated)
                                    .corner_radius(8.0)
                                    .inner_margin(egui::Margin::symmetric(10, 6))
                                    .show(ui, |ui| {
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
                                            ui.label(RichText::new(name).strong());
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    if !ready {
                                                        let can_download = can_start_model_download(
                                                            connected, active, busy,
                                                        );
                                                        if ui
                                                            .add_enabled(
                                                                can_download,
                                                                egui::Button::new("Download")
                                                                    .small(),
                                                            )
                                                            .clicked()
                                                        {
                                                            self.post(
                                                                "/api/models/download",
                                                                json!({
                                                                    "backend": self.backend,
                                                                    "selections": {key: name}
                                                                }),
                                                            );
                                                        }
                                                    }
                                                },
                                            );
                                        });
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
            components::copyable_path(
                ui,
                path,
                "Choose a model location in Settings, then run setup.",
            );
            ui.add_space(theme::space::XS);
            ui.label(RichText::new(if self.model_storage == "wsl" {
                "Weights are installed in Ubuntu under WSL. The Windows app and Python engine remain on Windows."
            } else {
                "Weights are installed on this Windows PC alongside the app’s local data."
            }).size(12.0).color(palette.secondary));
        });
    }

    fn model_library_page(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        if self.library.is_null() && !self.library_loading {
            self.library_loading = true;
            self.get("/api/library/models");
        }
        let mut search_requested = false;
        let mut refresh_requested = false;
        Self::card(ui, "Explore the model community", |ui| {
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
        Self::card(ui, "Community models", |ui| {
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
        if preview.is_some()
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

    fn benchmarks_page(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let categories = [
            ("upscale", "Upscale"),
            ("interpolate", "Interpolate"),
            ("decompress", "Decompress"),
            ("denoise", "Denoise"),
            ("deblur", "Deblur"),
        ];
        let resolutions = ["480p", "720p", "1080p", "1440p", "4K"];
        let running = self.benchmarks["status"].as_str() == Some("running");
        let mut start_payload = None;
        let available_backends = self.bootstrap["system"]["available_backends"].clone();
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
        let result_count = self.benchmarks["results"].as_array().map_or(0, |items| {
            items
                .iter()
                .filter(|item| item["status"] == "complete")
                .count()
        });

        Self::card(ui, "Run a local benchmark", |ui| {
            ui.label(RichText::new(
                "REAL Video Enhancer runs its actual inference pipeline against a repeatable synthetic 24 fps clip. No processed video is saved.",
            ).color(palette.secondary));
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Backend").strong());
                egui::ComboBox::from_id_salt("benchmark-backend")
                    .selected_text(self.backend.to_uppercase())
                    .show_ui(ui, |ui| {
                        for backend in ["pytorch", "ncnn", "tensorrt"] {
                            let ready = backend_is_installed(&available_backends, backend);
                            let label = if ready {
                                backend.to_uppercase()
                            } else {
                                format!("{} · unavailable", backend.to_uppercase())
                            };
                            if ui
                                .add_enabled(
                                    ready && !running,
                                    egui::Button::selectable(self.backend == backend, label),
                                )
                                .clicked()
                            {
                                self.backend = backend.to_owned();
                                self.save_render_defaults();
                                self.get(&format!("/api/models?backend={backend}"));
                            }
                        }
                    });
                ui.label(RichText::new("Category").strong());
                egui::ComboBox::from_id_salt("benchmark-category")
                    .selected_text(if self.benchmark_category == "all" {
                        "All categories"
                    } else {
                        categories
                            .iter()
                            .find(|(key, _)| *key == self.benchmark_category)
                            .map_or("All categories", |(_, label)| *label)
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.benchmark_category,
                            "all".to_owned(),
                            "All categories",
                        );
                        for (key, label) in categories {
                            ui.selectable_value(
                                &mut self.benchmark_category,
                                key.to_owned(),
                                label,
                            );
                        }
                    });
                ui.label(RichText::new("Clip length").strong());
                egui::ComboBox::from_id_salt("benchmark-frames")
                    .selected_text(format!(
                        "{} frames · {:.0} sec",
                        self.benchmark_frames,
                        self.benchmark_frames as f32 / 24.0
                    ))
                    .show_ui(ui, |ui| {
                        for frames in [24_usize, 48, 96] {
                            ui.selectable_value(
                                &mut self.benchmark_frames,
                                frames,
                                format!("{} frames · {:.0} sec", frames, frames as f32 / 24.0),
                            );
                        }
                    });
            });

            ui.add_space(theme::space::MD);
            ui.label(RichText::new("Input resolution").strong());
            ui.horizontal_wrapped(|ui| {
                for resolution in resolutions {
                    let selected = self
                        .benchmark_resolutions
                        .entry(resolution.to_owned())
                        .or_insert(false);
                    let response = ui.add_sized(
                        Vec2::new(82.0, 36.0),
                        egui::Button::new(resolution)
                            .fill(if *selected {
                                palette.accent.gamma_multiply(0.18)
                            } else {
                                palette.elevated
                            })
                            .stroke(Stroke::new(
                                if *selected { 1.5 } else { 1.0 },
                                if *selected {
                                    palette.accent
                                } else {
                                    palette.border
                                },
                            )),
                    );
                    if response.clicked() && !running {
                        *selected = !*selected;
                    }
                }
            });
            ui.add_space(theme::space::SM);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{installed_count} installed models · {result_count} saved measurements"
                    ))
                    .color(palette.secondary),
                );
                ui.label(
                    RichText::new("4K upscale tests use 256 px tiles to limit memory use.")
                        .size(11.0)
                        .color(palette.muted),
                );
                if self.model_storage == "wsl" && ui.button("Scan Ubuntu library").clicked() {
                    self.show_toast(
                        "Scanning WSL models",
                        "Ubuntu will stay available for the benchmark and other WSL tasks.",
                        ToastKind::Info,
                    );
                    self.get(&format!(
                        "/api/models?backend={}&refresh_wsl=1",
                        self.backend
                    ));
                }
                let gpu_id = if self.backend == "ncnn" {
                    self.ncnn_gpu_id
                } else {
                    self.pytorch_gpu_id
                };
                ui.label(
                    RichText::new(format!(
                        "Profile · {} · GPU {gpu_id} · 256 px tiles",
                        self.precision
                    ))
                    .size(11.0)
                    .color(palette.muted),
                );
            });
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                let selected_resolutions = resolutions
                    .iter()
                    .filter(|resolution| {
                        self.benchmark_resolutions
                            .get(**resolution)
                            .copied()
                            .unwrap_or(false)
                    })
                    .map(|resolution| (*resolution).to_owned())
                    .collect::<Vec<_>>();
                let selected_categories = categories
                    .iter()
                    .filter(|(key, _)| {
                        self.benchmark_category == "all" || self.benchmark_category == *key
                    })
                    .map(|(key, _)| (*key).to_owned())
                    .collect::<Vec<_>>();
                let can_start = !running
                    && !self.busy
                    && self.connected
                    && backend_is_installed(&available_backends, &self.backend)
                    && !selected_resolutions.is_empty()
                    && installed_count > 0;
                if ui
                    .add_enabled(
                        can_start,
                        egui::Button::new("Run selected benchmarks")
                            .fill(palette.accent)
                            .stroke(Stroke::NONE),
                    )
                    .clicked()
                {
                    start_payload = Some(json!({
                        "backend": self.backend,
                        "categories": selected_categories,
                        "resolutions": selected_resolutions,
                        "frames": self.benchmark_frames,
                        "precision": self.precision,
                        "pytorch_gpu_id": self.pytorch_gpu_id,
                        "ncnn_gpu_id": self.ncnn_gpu_id,
                    }));
                }
                if running && ui.button("Cancel benchmark").clicked() {
                    self.post("/api/benchmarks/cancel", json!({}));
                }
                if !self.connected {
                    ui.label(RichText::new("Waiting for the local engine.").color(palette.warning));
                } else if !backend_is_installed(&available_backends, &self.backend) {
                    let detail = if self.backend == "tensorrt" {
                        "TensorRT needs a CUDA-capable NVIDIA GPU and the complete TensorRT runtime. Open Runtime to install it; otherwise choose PyTorch or NCNN."
                    } else {
                        "This backend is not installed. Open Runtime to install it or choose an available backend."
                    };
                    ui.label(RichText::new(detail).color(palette.warning));
                } else if installed_count == 0 {
                    ui.label(
                        RichText::new("Install models from Downloads before benchmarking.")
                            .color(palette.warning),
                    );
                } else if selected_resolutions.is_empty() {
                    ui.label(
                        RichText::new("Select at least one input resolution.")
                            .color(palette.warning),
                    );
                }
            });

            if running
                || self.benchmarks["status"]
                    .as_str()
                    .is_some_and(|status| matches!(status, "completed" | "cancelled" | "error"))
            {
                ui.add_space(theme::space::MD);
                let stage = self.benchmarks["stage"]
                    .as_str()
                    .unwrap_or("Benchmark status");
                ui.horizontal(|ui| {
                    components::status_dot(
                        ui,
                        if self.benchmarks["status"] == "error" {
                            palette.error
                        } else if running {
                            palette.accent
                        } else {
                            palette.success
                        },
                        stage,
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.benchmarks["total"].as_u64().unwrap_or(0) > 0 {
                            ui.label(format!(
                                "{} / {}",
                                self.benchmarks["completed"].as_u64().unwrap_or(0),
                                self.benchmarks["total"].as_u64().unwrap_or(0)
                            ));
                        }
                    });
                });
                if running {
                    ui.add(
                        egui::ProgressBar::new(
                            self.benchmarks["progress"].as_f64().unwrap_or(0.0) as f32 / 100.0,
                        )
                        .show_percentage(),
                    );
                }
                ui.label(
                    RichText::new(self.benchmarks["detail"].as_str().unwrap_or_default())
                        .size(12.0)
                        .color(palette.secondary),
                );
                if let Some(current) = self.benchmarks["current"].as_object() {
                    let model = current
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or("Model");
                    let resolution = current
                        .get("resolution")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if let Some(fps) = current.get("engine_reported_fps").and_then(Value::as_f64) {
                        ui.label(
                            RichText::new(format!("{model} · {resolution} · {fps:.1} live fps"))
                                .strong()
                                .color(palette.accent),
                        );
                    }
                }
                if let Some(error) = self.benchmarks["error"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                {
                    ui.colored_label(palette.error, error);
                }
            }
        });
        if let Some(payload) = start_payload {
            self.post("/api/benchmarks/start", payload);
        }

        ui.add_space(theme::space::MD);
        Self::card(ui, "Model performance by category", |ui| {
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
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Hardware · {}",
                    self.bootstrap["system"]["gpu_name"]
                        .as_str()
                        .unwrap_or("GPU not detected")
                ))
                .strong(),
            );
            ui.label(
                RichText::new(format!(
                    "{} · {} RAM",
                    self.bootstrap["system"]["cpu"]
                        .as_str()
                        .unwrap_or("CPU unavailable"),
                    self.bootstrap["system"]["ram"]
                        .as_str()
                        .unwrap_or("RAM unavailable")
                ))
                .size(11.0)
                .color(palette.secondary),
            );
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
            || (progress > 0.0
                && progress < 100.0
                && !matches!(status.as_str(), "cancelled" | "error" | "completed"));
        let failed = status == "error";
        let cancelled = status == "cancelled";
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
        let managed_python = self.python["python_managed"].as_bool() == Some(true);
        let runtime_ready = !available.starts_with("No ") && managed_python;

        Self::card(ui, "Inference runtime", |ui| {
            ui.horizontal(|ui| {
                components::status_dot(
                    ui,
                    if installing {
                        palette.accent
                    } else if failed || cancelled || available.starts_with("No ") {
                        palette.warning
                    } else {
                        palette.success
                    },
                    if installing {
                        "Installing"
                    } else if failed {
                        "Setup failed"
                    } else if cancelled {
                        "Setup cancelled"
                    } else if !runtime_ready {
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
            if let Some(issue) = self.bootstrap["system"]["backend_probe_output"]
                .as_str()
                .filter(|issue| issue.starts_with("TensorRT package versions do not match:"))
            {
                ui.label(RichText::new(issue).size(11.0).color(palette.warning));
                ui.label(RichText::new("Run the recommended runtime setup to restore PyTorch 2.8, then install TensorRT again if it is still unavailable.")
                    .size(11.0).color(palette.secondary));
            }
            if installing {
                ui.add(
                    egui::ProgressBar::new(progress / 100.0).text(
                        self.install["stage"]
                            .as_str()
                            .unwrap_or("Preparing runtime"),
                    ),
                );
            }
            if installing
                || self.install["output"]
                    .as_array()
                    .is_some_and(|lines| !lines.is_empty())
            {
                ui.add_space(theme::space::SM);
                egui::Frame::new()
                    .fill(Color32::from_rgb(9, 12, 17))
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(theme::radius::CONTROL)
                    .inner_margin(egui::Margin::symmetric(12, 9))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Live setup output")
                                    .strong()
                                    .color(Color32::from_rgb(228, 234, 241)),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if installing {
                                    components::status_dot(ui, palette.success, "LIVE");
                                } else if failed {
                                    components::status_dot(
                                        ui,
                                        palette.error,
                                        "Finished with errors",
                                    );
                                } else if cancelled {
                                    components::status_dot(ui, palette.warning, "Cancelled");
                                } else {
                                    components::status_dot(ui, palette.muted, "Last run");
                                }
                            });
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt("runtime-live-output-terminal")
                            .max_height(if installing { 250.0 } else { 180.0 })
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                if let Some(lines) = self.install["output"].as_array() {
                                    for line in
                                        lines.iter().rev().take(120).rev().filter_map(Value::as_str)
                                    {
                                        let lower = line.to_ascii_lowercase();
                                        let color = if lower.contains("error")
                                            || lower.contains("failed")
                                        {
                                            Color32::from_rgb(255, 123, 133)
                                        } else if lower.starts_with("$")
                                            || lower.contains("installing")
                                        {
                                            Color32::from_rgb(137, 199, 255)
                                        } else {
                                            Color32::from_rgb(188, 198, 210)
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(line)
                                                    .monospace()
                                                    .size(10.5)
                                                    .color(color),
                                            )
                                            .wrap(),
                                        );
                                    }
                                } else {
                                    ui.label(
                                        RichText::new("Waiting for setup output…")
                                            .monospace()
                                            .size(10.5)
                                            .color(Color32::from_rgb(145, 157, 172)),
                                    );
                                }
                            });
                    });
            }
            if failed || cancelled {
                ui.add_space(theme::space::SM);
                egui::Frame::new()
                    .fill(palette.error.gamma_multiply(0.10))
                    .stroke(Stroke::new(1.0, palette.error.gamma_multiply(0.35)))
                    .corner_radius(theme::radius::CONTROL)
                    .inner_margin(egui::Margin::symmetric(12, 9))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(if failed {
                                "Setup needs attention"
                            } else {
                                "Setup was cancelled"
                            })
                            .strong()
                            .color(if failed {
                                palette.error
                            } else {
                                palette.warning
                            }),
                        );
                    });
            }
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                let enabled = !installing && !self.busy;
                let enabled = enabled && self.benchmarks["status"].as_str() != Some("running");
                if ui
                    .add_enabled(enabled, egui::Button::new("Install recommended runtime"))
                    .clicked()
                {
                    self.start_recommended_runtime_setup();
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
                if installing && ui.button("Cancel setup").clicked() {
                    self.post("/api/actions/cancel", json!({}));
                }
            });
        });

        ui.add_space(theme::space::MD);
        Self::card(ui, "Hardware and compatibility", |ui| {
            let metrics = [
                (
                    "Operating system",
                    system["os"].as_str().unwrap_or("Not detected").to_owned(),
                ),
                (
                    "Graphics device",
                    system["gpu_name"]
                        .as_str()
                        .unwrap_or("Not detected")
                        .to_owned(),
                ),
                (
                    "CUDA version",
                    system["cuda_version"]
                        .as_str()
                        .unwrap_or("Not detected")
                        .to_owned(),
                ),
                ("Available backends", available.clone()),
            ];
            let draw_metric = |ui: &mut egui::Ui, label: &str, value: &str| {
                egui::Frame::new()
                    .fill(palette.elevated)
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(12, 9))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(RichText::new(label).size(11.0).color(palette.muted));
                        ui.label(RichText::new(value).strong().color(palette.text));
                    });
            };
            if runtime_metrics_use_two_columns(ui.available_width()) {
                for row in metrics.chunks_exact(2) {
                    ui.columns(2, |columns| {
                        for (column, (label, value)) in columns.iter_mut().zip(row.iter()) {
                            draw_metric(column, label, value);
                        }
                    });
                    ui.add_space(theme::space::SM);
                }
            } else {
                for (label, value) in &metrics {
                    draw_metric(ui, label, value);
                }
            }
            if ui.button("Refresh system detection").clicked() {
                self.post("/api/actions/refresh_backends", json!({}));
                self.get("/api/bootstrap");
            }
        });

        ui.add_space(theme::space::MD);
        Self::card(ui, "AnimeScripter engine", |ui| {
            let ready = self.anime["ready"].as_bool() == Some(true);
            ui.horizontal_wrapped(|ui| {
                components::status_dot(
                    ui,
                    if ready {
                        palette.success
                    } else if self.install["status"] == "error" {
                        palette.error
                    } else {
                        palette.warning
                    },
                    if ready {
                        "Installed and verified"
                    } else {
                        "Optional · not installed"
                    },
                );
                let installing = self.install["status"].as_str() == Some("running");
                if ui
                    .add_enabled(
                        self.connected && !installing && self.active_task().is_none(),
                        egui::Button::new(if ready {
                            "Repair AnimeScripter"
                        } else {
                            "Install AnimeScripter CLI"
                        }),
                    )
                    .clicked()
                {
                    self.post("/api/animescripter/install", json!({}));
                }
                if installing && ui.button("Cancel setup").clicked() {
                    self.post("/api/actions/cancel", json!({}));
                }
                if ui
                    .hyperlink_to(
                        "Official AnimeScripter project ↗",
                        "https://github.com/NevermindNilas/TheAnimeScripter",
                    )
                    .clicked()
                {}
            });
            ui.label("Select AnimeScripter in the Process page to use its anime-focused upscale, RIFE interpolation, and restoration pipeline. Its optional model weights download on first use.");
            if let Some(detail) = self.anime["detail"]
                .as_str()
                .filter(|detail| !detail.is_empty())
            {
                ui.label(RichText::new(detail).size(11.0).color(palette.secondary));
            }
            if let Some(path) = self.anime["executable"].as_str() {
                components::copyable_path(ui, path, "AnimeScripter CLI path");
            }
            if self.install["status"] == "error" {
                if let Some(error) = self.install["error"].as_str() {
                    ui.colored_label(palette.error, error);
                }
            }
            if let Some(lines) = self.install["output"].as_array() {
                egui::CollapsingHeader::new("Setup log")
                    .id_salt("animescripter-setup-log")
                    .show(ui, |ui| {
                        for line in lines.iter().rev().take(12).filter_map(Value::as_str) {
                            ui.label(RichText::new(line).monospace().size(10.0));
                        }
                    });
            }
        });
    }

    fn python_page(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let python_info_pending = self.connected && self.python.is_null();
        let python_installing = self.install["status"].as_str() == Some("running");
        let interpreter_available = self.python["python_available"].as_bool().unwrap_or(false);
        let interpreter_present = self.python["python_present"].as_bool().unwrap_or(false);
        let available = self.python["python_managed"].as_bool() == Some(true);
        let benchmark_running = self.benchmarks["status"].as_str() == Some("running");
        let install_failed = self.install["status"].as_str() == Some("error");
        let python_path = self.python["python_path"].as_str().unwrap_or("").to_owned();
        Self::card(ui, "Managed Python", |ui| {
            ui.horizontal(|ui| {
                components::status_dot(
                    ui,
                    if available {
                        palette.success
                    } else if interpreter_present || install_failed {
                        palette.error
                    } else {
                        palette.warning
                    },
                    if available {
                        "Interpreter ready"
                    } else if interpreter_present {
                        "Found, but could not start"
                    } else if install_failed {
                        "Setup failed"
                    } else {
                        "Not installed"
                    },
                );
                if ui.button("Refresh environment").clicked() {
                    self.get("/api/python/info");
                }
            });
            ui.label(if python_info_pending {
                "Checking the app-managed Python environment…"
            } else if python_installing {
                "Setting up the app-managed Python runtime…"
            } else {
                self.python["python_detail"]
                    .as_str()
                    .unwrap_or("The Runtime page prepares the local Python engine.")
            });
            if !available {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(if install_failed {
                            "Retry runtime setup"
                        } else if interpreter_present {
                            "Repair Python runtime"
                        } else {
                            "Set up Python runtime"
                        })
                        .clicked()
                    {
                        self.page = RUNTIME_PAGE;
                        self.start_recommended_runtime_setup();
                    }
                    if install_failed {
                        ui.label(
                            RichText::new(
                                self.install["detail"]
                                    .as_str()
                                    .unwrap_or("Runtime setup failed."),
                            )
                            .color(palette.error),
                        );
                    } else if interpreter_available {
                        ui.label(
                            RichText::new(
                                "A system Python was detected; setup will create the app-managed runtime.",
                            )
                            .color(palette.secondary),
                        );
                    }
                });
            }
            ui.add_space(theme::space::SM);
            ui.label(
                RichText::new("Interpreter path")
                    .size(11.0)
                    .color(palette.muted),
            );
            components::copyable_path(
                ui,
                &python_path,
                if python_info_pending {
                    "Checking interpreter…"
                } else if python_installing {
                    "Installing Python…"
                } else if interpreter_present {
                    "The interpreter was found, but Windows could not start it. Repair the runtime from this page."
                } else if self.connected {
                    "App-managed Python is not installed"
                } else if self.engine_error.is_some() {
                    "Engine unavailable"
                } else {
                    "Waiting for engine…"
                },
            );
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                for (command, label) in [
                    ("--version", "Python version"),
                    ("list", "Installed packages"),
                    ("freeze", "Full package list"),
                ] {
                    if ui
                        .add_enabled(
                            available && !self.busy && !benchmark_running,
                            egui::Button::new(label),
                        )
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

    fn appearance_settings(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        Self::card(ui, "Appearance", |ui| {
            ui.label(
                RichText::new("Choose a comfortable look for long editing sessions.")
                    .size(12.0)
                    .color(palette.secondary),
            );
            ui.add_space(theme::space::SM);
            ui.label(
                RichText::new("Theme")
                    .size(11.0)
                    .strong()
                    .color(palette.muted),
            );
            ui.horizontal(|ui| {
                for (label, value) in [("Light", "light"), ("Dark", "dark")] {
                    let selected = self.appearance == value;
                    let response = ui.add_sized(
                        Vec2::new(90.0, 38.0),
                        egui::Button::new(label)
                            .fill(if selected {
                                palette.accent.gamma_multiply(0.16)
                            } else {
                                palette.elevated
                            })
                            .stroke(Stroke::new(
                                if selected { 1.4 } else { 1.0 },
                                if selected {
                                    palette.accent
                                } else {
                                    palette.border
                                },
                            )),
                    );
                    if response.clicked() {
                        self.appearance = value.to_owned();
                        update_ui_setting("appearance", json!(self.appearance));
                        apply_theme(ui.ctx(), self.accent, &self.appearance, self.reduce_motion);
                    }
                }
            });
            ui.add_space(theme::space::MD);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Reduce motion").strong());
                    ui.label(
                        RichText::new("Shorten transitions and loading animation.")
                            .size(11.0)
                            .color(palette.secondary),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if components::toggle(
                        ui,
                        &mut self.reduce_motion,
                        "Reduce motion",
                        self.accent,
                        true,
                    )
                    .changed()
                    {
                        update_ui_setting("reduce_motion", json!(self.reduce_motion));
                        apply_theme(ui.ctx(), self.accent, &self.appearance, self.reduce_motion);
                    }
                });
            });
            ui.add_space(theme::space::MD);
            ui.label(
                RichText::new("Accent color")
                    .size(11.0)
                    .strong()
                    .color(palette.muted),
            );
            ui.horizontal_wrapped(|ui| {
                for (label, rgb) in theme::ACCENTS {
                    let color = color_from_array(rgb);
                    let selected = self.accent == color;
                    let response = ui.add_sized(
                        Vec2::new(88.0, 38.0),
                        egui::Button::new(label)
                            .fill(palette.elevated)
                            .stroke(Stroke::new(
                                if selected { 1.6 } else { 1.0 },
                                if selected { color } else { palette.border },
                            )),
                    );
                    ui.painter().circle_filled(
                        egui::pos2(response.rect.left() + 11.0, response.rect.center().y),
                        4.0,
                        color,
                    );
                    if response.clicked() {
                        self.accent = color;
                        save_accent(color);
                        apply_theme(ui.ctx(), color, &self.appearance, self.reduce_motion);
                    }
                    response.on_hover_text(format!("Use the {label} accent"));
                }
            });
            ui.add_space(theme::space::SM);
            egui::Frame::new()
                .fill(palette.elevated)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(theme::radius::CONTROL)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.color_edit_button_srgba(&mut self.accent).changed() {
                            save_accent(self.accent);
                            apply_theme(
                                ui.ctx(),
                                self.accent,
                                &self.appearance,
                                self.reduce_motion,
                            );
                        }
                        ui.label(
                            RichText::new("Custom accent")
                                .size(11.0)
                                .color(palette.secondary),
                        );
                    });
                });
            if self.reduce_motion {
                ui.add_space(theme::space::SM);
                ui.label(
                    RichText::new("Reduced motion is on.")
                        .size(11.0)
                        .color(palette.secondary),
                );
            }
            ui.label(
                RichText::new("Your accent color is saved on this device.")
                    .size(11.0)
                    .color(palette.secondary),
            );
        });
    }

    fn model_storage_settings(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        Self::card(ui, "Model storage", |ui| {
            ui.label("Choose where model weights are installed and read from.");
            egui::ComboBox::from_label("Storage location")
                .selected_text(if self.model_storage == "wsl" {
                    "WSL Ubuntu"
                } else {
                    "This PC · Windows"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.model_storage, "wsl".to_owned(), "WSL Ubuntu");
                    ui.selectable_value(
                        &mut self.model_storage,
                        "windows".to_owned(),
                        "This PC · Windows",
                    );
                });
            if ui
                .add_enabled(
                    self.benchmarks["status"].as_str() != Some("running"),
                    egui::Button::new("Apply and install or verify all models"),
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
            if self.first_setup {
                ui.label("First launch: choose where to keep the models, then start setup. WSL starts automatically if selected.");
            }
            let status = self.storage_status["status"]
                .as_str()
                .unwrap_or("waiting")
                .to_owned();
            if matches!(status.as_str(), "starting" | "installing")
                && ui.button("Cancel model setup").clicked()
            {
                self.post("/api/actions/cancel", json!({}));
            }
            let stage = self.storage_status["stage"]
                .as_str()
                .unwrap_or("Waiting for engine status")
                .to_owned();
            let detail = self.storage_status["detail"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            ui.add_space(6.0);
            if status == "ready" {
                let location = if self.model_storage == "wsl" {
                    "WSL Ubuntu"
                } else {
                    "Windows"
                };
                components::status_dot(ui, palette.success, &format!("Ready · {location}"));
            } else {
                ui.label(format!("{stage} · {status}"));
            }
            if status != "ready" && !detail.is_empty() {
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
            components::copyable_path(
                ui,
                self.storage_status["model_dir"]
                    .as_str()
                    .unwrap_or_default(),
                "Storage path will be shown after setup",
            );
        });
    }

    fn update_settings(&mut self, ui: &mut egui::Ui) {
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
            ui.add_space(theme::space::SM);
            ui.label(RichText::new("Private repository access · optional").strong());
            ui.label(RichText::new("The app uses a saved token or your existing GitHub CLI login for private releases. Public releases need no sign-in. Saved tokens are encrypted for this Windows account.").size(11.0).color(secondary_text_color(!ui.visuals().dark_mode)));
            ui.horizontal_wrapped(|ui| {
                ui.add_sized(
                    Vec2::new(ui.available_width().min(360.0), 36.0),
                    egui::TextEdit::singleline(&mut self.update_token_input)
                        .password(true)
                        .hint_text("github_pat_… · read-only Contents access"),
                );
                if ui
                    .add_enabled(
                        !self.update_token_input.trim().is_empty(),
                        egui::Button::new("Save token securely"),
                    )
                    .clicked()
                {
                    let token = std::mem::take(&mut self.update_token_input);
                    self.post("/api/app/update/auth", json!({"token": token}));
                }
                if self.update["has_github_token"].as_bool() == Some(true)
                    && ui.button("Remove token").clicked()
                {
                    self.post("/api/app/update/auth", json!({"token": ""}));
                }
                ui.hyperlink_to(
                    "Create read-only token ↗",
                    "https://github.com/settings/personal-access-tokens/new",
                );
            });
            if !self.update_auth_message.is_empty() {
                ui.label(RichText::new(&self.update_auth_message).size(11.0).color(
                    if self.update_auth_message.contains("could not") {
                        theme::palette(!ui.visuals().dark_mode, self.accent).error
                    } else {
                        theme::palette(!ui.visuals().dark_mode, self.accent).success
                    },
                ));
            }
            if self.update["status"] == "error" {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).error,
                    "Update check needs attention",
                );
            }
            if self.update["status"] == "downloaded" {
                ui.colored_label(
                    theme::palette(!ui.visuals().dark_mode, self.accent).success,
                    "Update downloaded · starting installation…",
                );
            }
            if self.update["status"] == "installing" {
                ui.label("Installing quietly and reopening REAL Video Enhancer…");
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
                let downloading = self.update["status"] == "downloading";
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
                    && self.update["status"] != "installing"
                    && ui
                        .add_enabled(
                            !self.other_task_active(),
                            egui::Button::new("Download & install"),
                        )
                        .clicked()
                {
                    self.post("/api/app/update/download", json!({}));
                }
                if downloading && ui.button("Cancel download").clicked() {
                    self.post("/api/actions/cancel", json!({}));
                }
            });
        });
    }

    fn data_management_settings(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        let cache_bytes = self.footprint["components"]
            .as_array()
            .and_then(|components| components.iter().find(|item| item["id"] == "uv_cache"))
            .and_then(|item| item["bytes"].as_u64())
            .unwrap_or(0);
        Self::card(ui, "Managed downloads", |ui| {
            ui.label("Remove app-managed Python packages or model weights to reclaim disk space. Your videos and the app itself stay in place.");
            ui.add_space(theme::space::SM);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.active_task().is_none() && !self.managed_data_pending,
                        egui::Button::new("Delete Python and packages"),
                    )
                    .clicked()
                {
                    self.managed_data_message = None;
                    self.destructive_confirmation = Some("python".to_owned());
                }
                if ui
                    .add_enabled(
                        self.active_task().is_none() && !self.managed_data_pending,
                        egui::Button::new("Delete models"),
                    )
                    .clicked()
                {
                    self.managed_data_message = None;
                    self.destructive_confirmation = Some("models".to_owned());
                }
                if ui
                    .add_enabled(
                        cache_bytes > 0
                            && self.active_task().is_none()
                            && !self.managed_data_pending,
                        egui::Button::new("Prune unused package cache"),
                    )
                    .on_hover_text(
                        "Keep the installed runtime and remove cache entries uv considers unused.",
                    )
                    .clicked()
                {
                    self.destructive_confirmation = Some("uv_cache".to_owned());
                }
            });
            ui.label(
                RichText::new(format!(
                    "uv download cache · {} · pruned packages may need downloading again later",
                    format_bytes(cache_bytes)
                ))
                .size(11.0)
                .color(palette.secondary),
            );
            if let Some(message) = self.managed_data_message.as_deref() {
                ui.add_space(theme::space::XS);
                ui.horizontal(|ui| {
                    if self.managed_data_pending {
                        ui.spinner();
                    }
                    ui.label(RichText::new(message).color(if self.managed_data_failed {
                        palette.error
                    } else if self.managed_data_pending {
                        palette.accent
                    } else {
                        palette.success
                    }));
                });
            }
            ui.label(RichText::new("Python can be installed again from Runtime. Models can be downloaded again from Downloads.").size(11.0).color(palette.secondary));
        });
    }

    fn storage_footprint_settings(&mut self, ui: &mut egui::Ui) {
        let palette = theme::palette(!ui.visuals().dark_mode, self.accent);
        if self.footprint.is_null() && self.connected && !self.footprint_loading {
            self.footprint_loading = true;
            self.get("/api/storage/footprint");
        }
        Self::card(ui, "App storage footprint", |ui| {
            ui.label(RichText::new("Windows runs the desktop app and inference runtime. The model library stays in WSL and selected weights are copied to Windows only for a video render.").size(12.0).color(palette.secondary));
            ui.add_space(theme::space::SM);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Measured managed storage · {}",
                        format_bytes(self.footprint["total_bytes"].as_u64().unwrap_or(0))
                    ))
                    .strong()
                    .size(16.0),
                );
                if let Some(free) = self.footprint["free_bytes"].as_u64() {
                    ui.label(
                        RichText::new(format!("Windows disk free · {}", format_bytes(free)))
                            .color(palette.secondary),
                    );
                }
                if self.footprint_loading {
                    ui.spinner();
                }
                if ui
                    .add_enabled(!self.footprint_loading, egui::Button::new("Refresh sizes"))
                    .clicked()
                {
                    self.footprint_loading = true;
                    self.get("/api/storage/footprint");
                }
            });
            ui.add_space(theme::space::XS);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "WINDOWS · {}",
                        format_bytes(self.footprint["windows_bytes"].as_u64().unwrap_or(0))
                    ))
                    .strong()
                    .color(palette.text),
                );
                ui.label(
                    RichText::new(if self.footprint["free_bytes"].as_u64().is_some() {
                        format!(
                            "{} free",
                            format_bytes(self.footprint["free_bytes"].as_u64().unwrap_or(0))
                        )
                    } else {
                        "Windows free space unavailable".to_owned()
                    })
                    .size(11.0)
                    .color(palette.secondary),
                );
            });
            if let Some(items) = self.footprint["components"].as_array() {
                for component in items {
                    if component["storage"].as_str().unwrap_or("windows") != "windows" {
                        continue;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(component["label"].as_str().unwrap_or("Managed data"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format_bytes(
                                    component["bytes"].as_u64().unwrap_or(0),
                                ))
                                .strong()
                                .color(palette.text),
                            );
                        });
                    });
                }
            }
            ui.add_space(theme::space::SM);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!(
                        "WSL UBUNTU · {}",
                        if self.model_storage != "wsl" {
                            "not selected".to_owned()
                        } else if self.footprint["wsl_size_known"].as_bool().unwrap_or(false) {
                            format_bytes(self.footprint["wsl_bytes"].as_u64().unwrap_or(0))
                        } else {
                            "model size not measured".to_owned()
                        }
                    ))
                    .strong()
                    .color(palette.text),
                );
                ui.label(
                    RichText::new(if self.model_storage != "wsl" {
                        "Not selected"
                    } else if self.footprint["wsl_running"].as_bool().unwrap_or(false) {
                        "Running"
                    } else {
                        "Ubuntu stopped"
                    })
                    .size(11.0)
                    .color(palette.secondary),
                );
                if let Some(free) = self.footprint["wsl_free_bytes"].as_u64() {
                    ui.label(
                        RichText::new(format!("{} free", format_bytes(free)))
                            .size(11.0)
                            .color(palette.secondary),
                    );
                }
            });
            if let Some(items) = self.footprint["components"].as_array() {
                for component in items {
                    if component["storage"].as_str().unwrap_or_default() != "wsl" {
                        continue;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(component["label"].as_str().unwrap_or("Managed WSL data"));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format_bytes(
                                    component["bytes"].as_u64().unwrap_or(0),
                                ))
                                .strong()
                                .color(palette.text),
                            );
                        });
                    });
                }
            }
            if !self.footprint["error"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
            {
                ui.colored_label(
                    palette.warning,
                    self.footprint["error"].as_str().unwrap_or_default(),
                );
            }
            ui.label(RichText::new(if self.model_storage != "wsl" {
                "Model storage is set to Windows. The managed model library stays on the Windows drive."
            } else if !self.footprint["wsl_size_known"].as_bool().unwrap_or(false) {
                "Ubuntu is stopped and its model size has not been measured yet. Storage checks do not start WSL. A video render stages only selected models temporarily, stops Ubuntu before inference, and removes the copy afterward."
            } else if self.footprint["wsl_measurement_cached"].as_bool().unwrap_or(false) {
                "Ubuntu is stopped; its model size is the last saved measurement. A render stages only selected models temporarily, then removes them when finished."
            } else {
                "Starting or downloading models can use WSL. Real video renders stage only selected weights in Windows, stop Ubuntu before inference, and remove the temporary copy when finished. Benchmarks keep their normal WSL behavior."
            }).size(11.0).color(palette.muted));
        });
    }

    fn render_default_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let mut render_defaults_changed = false;
        let mut preview_enabled = self.saved_settings["advanced"]["preview_enabled"]
            .as_bool()
            .unwrap_or(true);
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
                render_defaults_changed |= ui
                    .checkbox(&mut preview_enabled, "Live render preview")
                    .on_hover_text(
                        "Show a small, real frame from the RVE renderer while it is working.",
                    )
                    .changed();
            });
        });
        if render_defaults_changed {
            if !self.saved_settings["advanced"].is_object() {
                self.saved_settings["advanced"] = json!({});
            }
            self.saved_settings["advanced"]["preview_enabled"] = json!(preview_enabled);
        }
        render_defaults_changed
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let mut render_defaults_changed = false;
        if !settings_uses_two_columns(ui.available_width()) {
            if self.first_setup {
                self.model_storage_settings(ui);
                ui.add_space(theme::space::MD);
                self.appearance_settings(ui);
            } else {
                self.appearance_settings(ui);
                ui.add_space(theme::space::MD);
                self.model_storage_settings(ui);
            }
            ui.add_space(theme::space::MD);
            self.update_settings(ui);
            ui.add_space(theme::space::MD);
            render_defaults_changed = self.render_default_settings(ui);
            ui.add_space(theme::space::MD);
            self.data_management_settings(ui);
            ui.add_space(theme::space::MD);
            self.storage_footprint_settings(ui);
        } else {
            ui.columns(2, |columns| {
                if self.first_setup {
                    self.model_storage_settings(&mut columns[0]);
                    columns[0].add_space(theme::space::MD);
                    render_defaults_changed = self.render_default_settings(&mut columns[0]);
                    self.appearance_settings(&mut columns[1]);
                } else {
                    self.appearance_settings(&mut columns[0]);
                    columns[0].add_space(theme::space::MD);
                    render_defaults_changed = self.render_default_settings(&mut columns[0]);
                    self.model_storage_settings(&mut columns[1]);
                }
                columns[1].add_space(theme::space::MD);
                self.update_settings(&mut columns[1]);
                columns[1].add_space(theme::space::MD);
                self.data_management_settings(&mut columns[1]);
                columns[1].add_space(theme::space::MD);
                self.storage_footprint_settings(&mut columns[1]);
            });
        }
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
        #[cfg(windows)]
        self.custom_titlebar(root_ui, &ctx);
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
        if self.update["status"] == "downloaded" && !self.update_install_started {
            self.install_downloaded_update(&ctx);
        }
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
        let light = self.appearance == "light";
        let theme_accent = accent_for_theme(self.accent, light);
        let compact_sidebar = root_ui.available_width() < 820.0;
        let sidebar_width = if compact_sidebar { 76.0 } else { 220.0 };
        egui::Panel::left("navigation")
            .resizable(false)
            .default_size(sidebar_width)
            .min_size(sidebar_width)
            .max_size(sidebar_width)
            .frame(
                egui::Frame::new()
                    .fill(theme::palette(light, self.accent).sidebar)
                    .stroke(Stroke::new(1.0, border_color(light)))
                    .inner_margin(if compact_sidebar { 10 } else { 18 }),
            )
            .show(root_ui, |ui| {
                ui.add_space(4.0);
                if compact_sidebar {
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        Self::brand_mark(ui, 44.0);
                    });
                } else {
                    ui.horizontal(|ui| {
                        Self::brand_mark(ui, 46.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("REAL VIDEO")
                                    .size(12.0)
                                    .strong()
                                    .color(theme::palette(light, self.accent).text),
                            );
                            ui.label(
                                RichText::new(format!("ENHANCER · v{APP_VERSION}"))
                                    .size(9.0)
                                    .color(theme::palette(light, self.accent).muted),
                            );
                        });
                    });
                }
                ui.add_space(if compact_sidebar { 16.0 } else { 28.0 });
                if !compact_sidebar {
                    ui.label(
                        RichText::new("WORKSPACE")
                            .size(9.0)
                            .strong()
                            .color(theme::palette(light, self.accent).muted),
                    );
                    ui.add_space(7.0);
                }
                for (index, (label, icon)) in PAGES.iter().enumerate() {
                    let group = match index {
                        2 => Some("DISCOVER"),
                        5 => Some("ENGINE"),
                        7 => Some("PREFERENCES"),
                        _ => None,
                    };
                    if let Some(group) = group
                        && !compact_sidebar
                    {
                        ui.add_space(if index == 7 { 16.0 } else { 12.0 });
                        ui.label(
                            RichText::new(group)
                                .size(9.0)
                                .strong()
                                .color(theme::palette(light, self.accent).muted),
                        );
                        ui.add_space(6.0);
                    }
                    let selected = self.page == index;
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(
                            if compact_sidebar {
                                52.0
                            } else {
                                sidebar_width - 36.0
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
                        theme_accent.gamma_multiply(if light { 0.13 } else { 0.12 })
                    } else if response.hovered() {
                        theme::palette(light, self.accent)
                            .elevated
                            .gamma_multiply(hover)
                    } else {
                        Color32::TRANSPARENT
                    };
                    let painter = ui.painter().clone();
                    painter.rect_filled(rect, 10.0, fill);
                    if selected {
                        painter.rect_filled(
                            egui::Rect::from_center_size(
                                egui::pos2(rect.left() + 3.0, rect.center().y),
                                Vec2::new(3.0, 23.0),
                            ),
                            1.5,
                            theme_accent,
                        );
                    }
                    let icon_color = if selected {
                        theme::palette(light, self.accent).accent
                    } else {
                        secondary_text_color(light)
                    };
                    let icon_center = if compact_sidebar {
                        rect.center()
                    } else {
                        egui::pos2(rect.left() + 22.0, rect.center().y)
                    };
                    ui.place(
                        egui::Rect::from_center_size(icon_center, Vec2::splat(22.0)),
                        icons::styled(icon, icon_color, if compact_sidebar { 22.0 } else { 20.0 }),
                    );
                    if !compact_sidebar {
                        painter.text(
                            egui::pos2(rect.left() + 45.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            *label,
                            egui::FontId::proportional(13.5),
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
                    ui.add_space(2.0);
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    let palette = theme::palette(light, self.accent);
                    let (state, color) = if self.connected {
                        ("Engine ready", palette.success)
                    } else if self.engine_error.is_some() {
                        ("Engine unavailable", palette.error)
                    } else {
                        ("Starting engine", palette.warning)
                    };
                    if compact_sidebar {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(48.0, 24.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, color);
                        let _ = response.on_hover_text(state);
                    } else {
                        components::status_dot(ui, color, state);
                    }
                });
            });

        let compact_content = root_ui.available_width() < 1120.0;
        if self.page == PROCESS_PAGE {
            let palette = theme::palette(light, self.accent);
            let footer_height = if root_ui.available_width() < 1200.0 {
                112.0
            } else {
                82.0
            };
            egui::Panel::bottom("process-render-bar")
                .resizable(false)
                .default_size(footer_height)
                .min_size(footer_height)
                .max_size(footer_height)
                .frame(
                    egui::Frame::new()
                        .fill(palette.sidebar)
                        .stroke(Stroke::new(1.0, palette.border))
                        .inner_margin(egui::Margin::symmetric(24, 10)),
                )
                .show(root_ui, |ui| self.render_action_bar(ui));
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::palette(light, self.accent).canvas)
                    .inner_margin(egui::Margin::symmetric(
                        if compact_content { 22 } else { 32 },
                        22,
                    )),
            )
            .show(root_ui, |ui| {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    if self.quit_when_idle {
                        let palette = theme::palette(light, self.accent);
                        egui::Frame::new()
                            .fill(palette.elevated)
                            .corner_radius(theme::radius::CONTROL)
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new("Will quit after current work finishes.")
                                            .color(palette.warning),
                                    );
                                    if ui.small_button("Cancel exit").clicked() {
                                        self.quit_when_idle = false;
                                    }
                                });
                            });
                        ui.add_space(theme::space::SM);
                    }
                    self.header(ui);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match self.page {
                            0 => self.home_page(ui),
                            1 => self.process_page(ui),
                            2 => self.downloads_page(ui),
                            3 => self.model_library_page(ui),
                            4 => self.benchmarks_page(ui),
                            5 => self.runtime_page(ui),
                            6 => self.python_page(ui),
                            7 => self.settings_page(ui),
                            _ => self.home_page(ui),
                        });
                });
            });

        if self.close_confirmation {
            let active_task = self.active_task().unwrap_or("A background task");
            egui::Window::new("Work is still running")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(&ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.label(RichText::new(format!("{active_task} is still in progress.")).strong());
                    ui.add_space(theme::space::SM);
                    ui.label("Keep REAL Video Enhancer running in the system tray? You can reopen it there while the work continues.");
                    if let Some(error) = &self.tray_error {
                        ui.add_space(theme::space::SM);
                        ui.colored_label(theme::palette(light, self.accent).error, error);
                    }
                    ui.add_space(theme::space::MD);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(self.tray_available(), egui::Button::new("Keep working in tray")).clicked() {
                            self.close_confirmation = false;
                            self.quit_when_idle = false;
                            self.hide_to_tray(&ctx);
                        }
                        if ui.button("Keep app open").clicked() {
                            self.close_confirmation = false;
                            self.quit_when_idle = false;
                        }
                        if ui.button("Cancel work and quit").clicked() {
                            self.close_confirmation = false;
                            self.quit_when_idle = true;
                            self.post("/api/actions/cancel", json!({}));
                        }
                    });
                });
        }

        if let Some(kind) = self.destructive_confirmation.clone() {
            let title = if kind == "python" {
                "Delete managed Python?"
            } else if kind == "uv_cache" {
                "Prune unused package cache?"
            } else {
                "Delete all managed models?"
            };
            let detail = if kind == "python" {
                "This removes the app-managed Python runtime and its installed packages. Runtime setup can install them again."
            } else if kind == "uv_cache" {
                "uv will prune package cache entries it considers unused. Your installed Python runtime and packages stay in place. Packages removed from the cache may need to be downloaded again during a future setup."
            } else {
                "This removes downloaded model weights, including custom models, from the selected Windows or WSL storage location. You can download them again later."
            };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(&ctx, |ui| {
                    ui.set_min_width(390.0);
                    ui.label(detail);
                    ui.add_space(theme::space::MD);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.destructive_confirmation = None;
                        }
                        if ui
                            .add_enabled(
                                self.active_task().is_none() && !self.managed_data_pending,
                                egui::Button::new(if kind == "uv_cache" {
                                    "Prune cache"
                                } else {
                                    "Delete files"
                                }),
                            )
                            .clicked()
                        {
                            self.destructive_confirmation = None;
                            if kind == "uv_cache" {
                                self.install = json!({
                                    "status": "running",
                                    "progress": 1,
                                    "stage": "Pruning unused package cache",
                                    "detail": "Removing package downloads uv marks unused.",
                                    "output": []
                                });
                                self.post("/api/storage/prune-cache", json!({}));
                            } else {
                                self.managed_data_kind = Some(kind.clone());
                                self.managed_data_message = Some(if kind == "python" {
                                    "Removing the managed Python environment, downloaded Python runtimes, and package cache…".to_owned()
                                } else if self.model_storage == "wsl" {
                                    "Removing the managed model library from WSL Ubuntu…".to_owned()
                                } else {
                                    "Removing the managed model library from this PC…".to_owned()
                                });
                                self.managed_data_failed = false;
                                self.managed_data_pending = true;
                                self.post("/api/data/delete", json!({"kind": kind.clone()}));
                            }
                        }
                    });
                });
        }

        self.draw_startup_animation(&ctx);
        self.draw_toast(&ctx);
    }
}

fn main() -> eframe::Result<()> {
    set_windows_app_identity();
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../docs/assets/rve-logo.png"))
        .unwrap_or_default();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("REAL Video Enhancer")
        .with_inner_size([1440.0, 920.0])
        .with_min_inner_size([860.0, 640.0])
        .with_resizable(true)
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
