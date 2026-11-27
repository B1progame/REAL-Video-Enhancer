//! Background threads that talk to the local engine.
//!
//! The UI thread never blocks: engine startup, polling, file dialogs, slow
//! requests and image decoding all happen here.

use std::{
    collections::HashMap,
    process::Child,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use eframe::egui;
use serde_json::Value;

use super::{api_get, api_get_image, api_post, render_preview_is_available, start_engine};

const PREVIEW_BOUNDS: (u32, u32) = (960, 540);
const THUMBNAIL_BOUNDS: (u32, u32) = (320, 214);
/// Connection failures this early are the engine still booting, not an error.
const STARTUP_GRACE: Duration = Duration::from_secs(30);

const POLLED: [&str; 7] = [
    "/api/render/status",
    "/api/models/download_status",
    "/api/startup/install_status",
    "/api/animescripter/status",
    "/api/storage/status",
    "/api/benchmarks/status",
    "/api/app/update/status",
];

#[derive(Clone)]
pub enum ApiCommand {
    Get(String),
    /// A user action that locks other actions until it answers.
    Post(String, Value),
    /// A POST that must never be blocked or block anything else.
    Query(String, Value),
}

pub struct ApiEvent {
    pub endpoint: String,
    pub value: Result<Value, String>,
    pub image: Option<Result<egui::ColorImage, String>>,
    pub is_post: bool,
}

pub struct ApiBridge {
    pub send: Sender<ApiCommand>,
    pub receive: Receiver<ApiEvent>,
    /// Set while the window is hidden, so polling can back off.
    pub hidden: Arc<AtomicBool>,
    pub engine: Arc<Mutex<Option<Child>>>,
}

#[derive(Clone)]
struct Outbox {
    send: Sender<ApiEvent>,
    ctx: egui::Context,
    /// Bumped on every POST so the poller re-delivers unchanged values and
    /// replaces any optimistic state the UI set.
    generation: Arc<AtomicU64>,
}

impl Outbox {
    fn push(&self, event: ApiEvent) {
        let _ = self.send.send(event);
        self.ctx.request_repaint();
    }

    fn value(&self, endpoint: impl Into<String>, value: Result<Value, String>, is_post: bool) {
        self.push(ApiEvent {
            endpoint: endpoint.into(),
            value,
            image: None,
            is_post,
        });
    }
}

pub fn decode_bounded(bytes: &[u8], bounds: (u32, u32)) -> Result<egui::ColorImage, String> {
    let decoded = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    let decoded = if decoded.width() > bounds.0 || decoded.height() > bounds.1 {
        decoded.thumbnail(bounds.0, bounds.1)
    } else {
        decoded
    };
    let rgba = decoded.into_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

fn image_bounds(endpoint: &str) -> Option<(u32, u32)> {
    if endpoint.starts_with("/api/library/thumbnail?id=") {
        Some(THUMBNAIL_BOUNDS)
    } else if endpoint.starts_with("/api/video/first_frame?") {
        Some(PREVIEW_BOUNDS)
    } else {
        None
    }
}

/// Requests that wait on a person (file dialogs) get their own thread.
fn waits_for_user(endpoint: &str) -> bool {
    endpoint.starts_with("/api/dialog/select") || endpoint.starts_with("/api/actions/import")
}

fn is_slow(endpoint: &str) -> bool {
    image_bounds(endpoint).is_some()
        || endpoint.starts_with("/api/storage/footprint")
        || endpoint.starts_with("/api/library/models")
        || endpoint.starts_with("/api/models/inventory")
        || endpoint == "/api/video/inspect"
        || endpoint == "/api/python/pip"
}

fn perform(client: &ureq::Agent, command: ApiCommand, outbox: &Outbox) {
    match command {
        ApiCommand::Get(endpoint) => match image_bounds(&endpoint) {
            Some(bounds) => {
                let image =
                    api_get_image(client, &endpoint).and_then(|b| decode_bounded(&b, bounds));
                outbox.push(ApiEvent {
                    endpoint,
                    value: Ok(Value::Null),
                    image: Some(image),
                    is_post: false,
                });
            }
            None => {
                let value = api_get(client, &endpoint);
                outbox.value(endpoint, value, false);
            }
        },
        ApiCommand::Post(endpoint, payload) => {
            let value = api_post(client, &endpoint, &payload);
            outbox.generation.fetch_add(1, Ordering::Relaxed);
            outbox.value(endpoint, value, true);
        }
        ApiCommand::Query(endpoint, payload) => {
            let value = api_post(client, &endpoint, &payload);
            outbox.generation.fetch_add(1, Ordering::Relaxed);
            outbox.value(endpoint, value, false);
        }
    }
}

struct Poller {
    connected: bool,
    ever_connected: bool,
    started: Instant,
    sent: HashMap<&'static str, Value>,
    generation: u64,
    preview_hash: u64,
    render_active: bool,
}

impl Poller {
    fn failed(&mut self, error: String, outbox: &Outbox) {
        self.connected = false;
        self.sent.clear();
        if self.ever_connected || self.started.elapsed() > STARTUP_GRACE {
            outbox.value("/api/connection", Err(error), false);
        }
    }

    fn poll(&mut self, client: &ureq::Agent, hidden: bool, outbox: &Outbox) {
        if !self.connected {
            match api_get(client, "/api/bootstrap") {
                Ok(value) => {
                    self.connected = true;
                    self.ever_connected = true;
                    outbox.value("/api/bootstrap", Ok(value), false);
                }
                Err(error) => self.failed(error, outbox),
            }
            return;
        }
        let generation = outbox.generation.load(Ordering::Relaxed);
        if generation != self.generation {
            self.generation = generation;
            self.sent.clear();
        }
        for endpoint in POLLED {
            let value = match api_get(client, endpoint) {
                Ok(value) => value,
                Err(error) => return self.failed(error, outbox),
            };
            let mut preview_wanted = false;
            if endpoint == "/api/render/status" {
                self.render_active = matches!(
                    value["status"].as_str(),
                    Some("preparing" | "running" | "paused")
                );
                preview_wanted = !hidden && render_preview_is_available(&value);
            }
            // Unchanged answers are not forwarded, so an idle app never repaints.
            if self.sent.get(endpoint) != Some(&value) {
                self.sent.insert(endpoint, value.clone());
                outbox.value(endpoint, Ok(value), false);
            }
            if preview_wanted {
                self.preview(client, outbox);
            }
        }
    }

    fn preview(&mut self, client: &ureq::Agent, outbox: &Outbox) {
        use std::hash::{Hash, Hasher};
        let Ok(bytes) = api_get_image(client, "/api/render/preview_frame") else {
            return;
        };
        if bytes.is_empty() {
            return;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        if hash == self.preview_hash {
            return;
        }
        self.preview_hash = hash;
        outbox.push(ApiEvent {
            endpoint: "/api/render/preview_frame".to_owned(),
            value: Ok(Value::Null),
            image: Some(decode_bounded(&bytes, PREVIEW_BOUNDS)),
            is_post: false,
        });
    }
}

pub fn start(ctx: egui::Context, model_storage: String) -> ApiBridge {
    let (command_send, command_receive) = mpsc::channel::<ApiCommand>();
    let (slow_send, slow_receive) = mpsc::channel::<ApiCommand>();
    let (event_send, event_receive) = mpsc::channel::<ApiEvent>();
    let outbox = Outbox {
        send: event_send,
        ctx,
        generation: Arc::new(AtomicU64::new(0)),
    };
    let hidden = Arc::new(AtomicBool::new(false));
    let engine = Arc::new(Mutex::new(None));

    let slow_outbox = outbox.clone();
    let _ = thread::Builder::new()
        .name("rve-slow".to_owned())
        .spawn(move || {
            let client = agent(Duration::from_secs(120));
            while let Ok(command) = slow_receive.recv() {
                perform(&client, command, &slow_outbox);
            }
        });

    let engine_slot = Arc::clone(&engine);
    let hidden_flag = Arc::clone(&hidden);
    let _ = thread::Builder::new()
        .name("rve-api".to_owned())
        .spawn(move || {
            match start_engine(&model_storage) {
                Ok(child) => {
                    if let Ok(mut slot) = engine_slot.lock() {
                        *slot = child;
                    }
                }
                Err(error) => outbox.value("/api/connection", Err(error), false),
            }
            // Commands queued before the port was chosen are still waiting.
            let client = agent(Duration::from_secs(25));
            let mut poller = Poller {
                connected: false,
                ever_connected: false,
                started: Instant::now(),
                sent: HashMap::new(),
                generation: 0,
                preview_hash: 0,
                render_active: false,
            };
            let mut next_refresh = Instant::now();
            loop {
                let wait = next_refresh.saturating_duration_since(Instant::now());
                match command_receive.recv_timeout(wait) {
                    Ok(command) => {
                        let endpoint = match &command {
                            ApiCommand::Get(endpoint)
                            | ApiCommand::Post(endpoint, _)
                            | ApiCommand::Query(endpoint, _) => endpoint.as_str(),
                        };
                        if waits_for_user(endpoint) {
                            let outbox = outbox.clone();
                            let _ = thread::Builder::new().name("rve-dialog".to_owned()).spawn(
                                move || {
                                    let client = agent(Duration::from_secs(3700));
                                    perform(&client, command, &outbox);
                                },
                            );
                        } else if is_slow(endpoint) {
                            let _ = slow_send.send(command);
                        } else {
                            let is_post = !matches!(command, ApiCommand::Get(_));
                            perform(&client, command, &outbox);
                            if is_post {
                                next_refresh = Instant::now();
                            }
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                if Instant::now() >= next_refresh {
                    let hidden = hidden_flag.load(Ordering::Relaxed);
                    poller.poll(&client, hidden, &outbox);
                    let interval = if !poller.connected {
                        400
                    } else if hidden {
                        3000
                    } else if poller.render_active {
                        800
                    } else {
                        1500
                    };
                    next_refresh = Instant::now() + Duration::from_millis(interval);
                }
            }
        });

    ApiBridge {
        send: command_send,
        receive: event_receive,
        hidden,
        engine,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_bounded, is_slow, waits_for_user};

    #[test]
    fn large_frames_are_downscaled_before_they_reach_the_gpu() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(3840, 2160)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let frame = decode_bounded(bytes.get_ref(), (960, 540)).unwrap();
        assert_eq!(frame.size, [960, 540]);
    }

    #[test]
    fn file_dialogs_never_share_the_polling_thread() {
        assert!(waits_for_user("/api/dialog/select_input"));
        assert!(waits_for_user("/api/actions/import_custom_models"));
        assert!(!waits_for_user("/api/render/status"));
        assert!(is_slow("/api/storage/footprint"));
        assert!(!is_slow("/api/render/stop"));
    }
}
