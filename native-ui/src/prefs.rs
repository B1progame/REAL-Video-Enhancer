//! Look-and-feel preferences, recent files, history, and the render queue.

use std::{collections::VecDeque, path::Path};

use eframe::egui::Color32;
use serde_json::{Value, json};

use super::{
    settings_path,
    ui::{backdrop::Material, theme},
    update_ui_setting,
};

pub const MAX_RECENT: usize = 8;
pub const MAX_HISTORY: usize = 20;

#[derive(Clone, PartialEq)]
pub struct Prefs {
    pub accent: Color32,
    pub appearance: String,
    pub reduce_motion: bool,
    pub material: Material,
    /// 0 is nearly solid, 1 is the most see-through.
    pub glass: f32,
    pub radius: u8,
    pub zoom: f32,
    pub glow: bool,
    pub recent: Vec<String>,
    pub history: Vec<HistoryEntry>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct HistoryEntry {
    pub input: String,
    pub output: String,
}

impl Prefs {
    pub fn load() -> Self {
        let config = std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
            .unwrap_or_else(|| json!({}));
        Self::from_value(&config)
    }

    pub fn from_value(config: &Value) -> Self {
        let preset = theme::ACCENTS[0].1;
        let accent = config["accent"]
            .as_array()
            .and_then(|values| {
                let rgb = values
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|channel| channel.min(255) as u8)
                    .collect::<Vec<_>>();
                match rgb.as_slice() {
                    [red, green, blue] => Some(Color32::from_rgb(*red, *green, *blue)),
                    _ => None,
                }
            })
            .unwrap_or_else(|| Color32::from_rgb(preset[0], preset[1], preset[2]));
        Self {
            accent,
            appearance: match config["appearance"].as_str() {
                Some("light") => "light".to_owned(),
                _ => "dark".to_owned(),
            },
            reduce_motion: config["reduce_motion"].as_bool().unwrap_or(false),
            material: Material::from_key(config["material"].as_str().unwrap_or("acrylic")),
            glass: (config["glass"].as_f64().unwrap_or(0.55) as f32).clamp(0.0, 1.0),
            radius: config["radius"].as_u64().unwrap_or(14).min(24) as u8,
            zoom: (config["zoom"].as_f64().unwrap_or(1.0) as f32).clamp(0.8, 1.4),
            glow: config["glow"].as_bool().unwrap_or(true),
            recent: config["recent"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .take(MAX_RECENT)
                .map(str::to_owned)
                .collect(),
            history: config["history"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    Some(HistoryEntry {
                        input: entry["input"].as_str()?.to_owned(),
                        output: entry["output"].as_str()?.to_owned(),
                    })
                })
                .take(MAX_HISTORY)
                .collect(),
        }
    }

    pub fn save_look(&self) {
        let [red, green, blue, _] = self.accent.to_array();
        update_ui_setting("accent", json!([red, green, blue]));
        update_ui_setting("appearance", json!(self.appearance));
        update_ui_setting("reduce_motion", json!(self.reduce_motion));
        update_ui_setting("material", json!(self.material.key()));
        update_ui_setting("glass", json!(self.glass));
        update_ui_setting("radius", json!(self.radius));
        update_ui_setting("zoom", json!(self.zoom));
        update_ui_setting("glow", json!(self.glow));
    }

    pub fn remember_recent(&mut self, path: &str) {
        if push_front_unique(&mut self.recent, path.to_owned(), MAX_RECENT) {
            update_ui_setting("recent", json!(self.recent));
        }
    }

    pub fn remember_render(&mut self, entry: HistoryEntry) {
        if push_front_unique(&mut self.history, entry, MAX_HISTORY) {
            self.save_history();
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.recent.clear();
        self.save_history();
        update_ui_setting("recent", json!(self.recent));
    }

    fn save_history(&self) {
        let history = self
            .history
            .iter()
            .map(|entry| json!({"input": entry.input, "output": entry.output}))
            .collect::<Vec<_>>();
        update_ui_setting("history", json!(history));
    }
}

/// Move `item` to the front. Returns whether the list changed.
fn push_front_unique<T: PartialEq>(list: &mut Vec<T>, item: T, limit: usize) -> bool {
    if list.first() == Some(&item) {
        return false;
    }
    list.retain(|existing| *existing != item);
    list.insert(0, item);
    list.truncate(limit);
    true
}

pub fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

pub fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            [
                "mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "flv", "ts", "m2ts", "mpg",
                "mpeg", "gif",
            ]
            .contains(&extension.to_ascii_lowercase().as_str())
        })
}

// ------------------------------------------------------------- render queue

#[derive(Default, Clone, PartialEq, Debug)]
pub enum QueuePhase {
    #[default]
    Stopped,
    /// Waiting for the current render, or ready to take the next file.
    Idle,
    /// The file was loaded into the editor and is being inspected.
    Loading,
    /// A start request was sent; waiting for the renderer to finish.
    Rendering,
}

#[derive(Default)]
pub struct RenderQueue {
    pub items: VecDeque<String>,
    pub phase: QueuePhase,
    pub finished: usize,
    pub note: Option<String>,
    /// The renderer has been seen working on the current file.
    started: bool,
}

#[derive(PartialEq, Debug)]
pub enum QueueAction {
    None,
    Load(String),
    Start,
}

/// What the app knows this frame, as far as the queue cares.
pub struct QueueInput<'a> {
    pub render_status: &'a str,
    /// The loaded file has been inspected and nothing blocks a render.
    pub ready: bool,
    /// Why a loaded file cannot render, if that is already certain.
    pub failure: Option<String>,
}

impl RenderQueue {
    pub fn add(&mut self, path: &str) -> bool {
        let path = path.trim();
        if path.is_empty() || self.items.iter().any(|item| item == path) {
            return false;
        }
        self.items.push_back(path.to_owned());
        true
    }

    pub fn running(&self) -> bool {
        self.phase != QueuePhase::Stopped
    }

    pub fn start(&mut self) {
        if !self.items.is_empty() && !self.running() {
            self.phase = QueuePhase::Idle;
            self.finished = 0;
            self.note = None;
        }
    }

    pub fn stop(&mut self, note: impl Into<String>) {
        self.phase = QueuePhase::Stopped;
        self.note = Some(note.into());
    }

    pub fn step(&mut self, input: &QueueInput) -> QueueAction {
        let busy = matches!(
            input.render_status,
            "preparing" | "running" | "paused" | "stopping"
        );
        match self.phase {
            QueuePhase::Stopped => QueueAction::None,
            QueuePhase::Idle => {
                if busy {
                    return QueueAction::None;
                }
                match self.items.front() {
                    Some(next) => {
                        self.phase = QueuePhase::Loading;
                        QueueAction::Load(next.clone())
                    }
                    None => {
                        let done = self.finished;
                        self.stop(format!("Queue finished · {done} video(s) rendered."));
                        QueueAction::None
                    }
                }
            }
            QueuePhase::Loading => {
                if let Some(reason) = &input.failure {
                    self.stop(format!("Queue paused: {reason}"));
                    QueueAction::None
                } else if input.ready {
                    self.phase = QueuePhase::Rendering;
                    self.started = false;
                    QueueAction::Start
                } else {
                    QueueAction::None
                }
            }
            QueuePhase::Rendering => {
                self.started |= busy;
                match input.render_status {
                    // "completed" may be left over from the file before.
                    "completed" if self.started => {
                        self.items.pop_front();
                        self.finished += 1;
                        self.phase = QueuePhase::Idle;
                    }
                    "error" => self.stop("Queue paused: the render failed."),
                    "stopped" | "cancelled" => self.stop("Queue paused: the render was cancelled."),
                    _ => {}
                }
                QueueAction::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(render_status: &str, ready: bool) -> QueueInput<'_> {
        QueueInput {
            render_status,
            ready,
            failure: None,
        }
    }

    #[test]
    fn queue_renders_each_file_in_turn_then_stops() {
        let mut queue = RenderQueue::default();
        assert!(queue.add("a.mp4"));
        assert!(queue.add("b.mp4"));
        assert!(!queue.add("a.mp4"));
        queue.start();
        for file in ["a.mp4", "b.mp4"] {
            assert_eq!(
                queue.step(&input("idle", false)),
                QueueAction::Load(file.into())
            );
            assert_eq!(queue.step(&input("idle", false)), QueueAction::None);
            assert_eq!(queue.step(&input("idle", true)), QueueAction::Start);
            queue.step(&input("running", false));
            queue.step(&input("completed", false));
        }
        queue.step(&input("completed", false));
        assert!(!queue.running());
        assert_eq!(queue.finished, 2);
        assert!(queue.items.is_empty());
    }

    #[test]
    fn a_leftover_completed_status_does_not_skip_the_next_file() {
        let mut queue = RenderQueue::default();
        queue.add("a.mp4");
        queue.start();
        queue.step(&input("idle", false));
        queue.step(&input("idle", true));
        // The previous render's answer is delivered again before this one begins.
        queue.step(&input("completed", false));
        assert_eq!(queue.items.len(), 1);
        queue.step(&input("preparing", false));
        queue.step(&input("completed", false));
        assert!(queue.items.is_empty());
    }

    #[test]
    fn a_failed_render_pauses_the_queue_and_keeps_the_file() {
        let mut queue = RenderQueue::default();
        queue.add("a.mp4");
        queue.start();
        queue.step(&input("idle", false));
        queue.step(&input("idle", true));
        queue.step(&input("error", false));
        assert!(!queue.running());
        assert_eq!(queue.items.len(), 1);
    }

    #[test]
    fn prefs_reject_bad_values_and_cap_lists() {
        let prefs = Prefs::from_value(&json!({
            "accent": [1, 2], "glass": 9.0, "zoom": 0.1, "radius": 200,
            "recent": ["1","2","3","4","5","6","7","8","9","10"]
        }));
        assert_eq!(prefs.accent, Color32::from_rgb(0, 190, 190));
        assert_eq!((prefs.glass, prefs.zoom, prefs.radius), (1.0, 0.8, 24));
        assert_eq!(prefs.recent.len(), MAX_RECENT);
    }

    #[test]
    fn video_extensions_are_recognised_case_insensitively() {
        assert!(is_video_path(Path::new("C:/a/Clip.MKV")));
        assert!(!is_video_path(Path::new("C:/a/notes.txt")));
    }
}
