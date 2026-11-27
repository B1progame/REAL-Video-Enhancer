//! Every long job in one place, so each gets the same bar, timing and ETA.

use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{Value, json};

use crate::{
    BENCHMARK_PAGE, NativeApp, PROCESS_PAGE, RUNTIME_PAGE, SETTINGS_PAGE, format_bytes,
    ui::{
        components,
        progress::{self, Activity, Timing},
        theme,
    },
};

const MODELS_PAGE: usize = 2;
const COMMUNITY_PAGE: usize = 3;

pub struct Job {
    pub activity: Activity,
    /// The engine's own estimate, preferred over ours when present.
    pub eta: Option<String>,
    /// False when progress moves in uneven steps and a forecast would mislead.
    pub forecast: bool,
    pub page: usize,
    pub cancel: Option<&'static str>,
}

/// Download speed from successive byte counts.
#[derive(Default)]
pub struct Rate {
    last: Option<(Instant, u64)>,
    pub bytes_per_second: f64,
}

impl Rate {
    pub fn sample(&mut self, bytes: u64, now: Instant) {
        match self.last {
            Some((then, before)) if bytes >= before => {
                let seconds = now.duration_since(then).as_secs_f64();
                if seconds < 0.5 {
                    return;
                }
                let current = (bytes - before) as f64 / seconds;
                self.bytes_per_second = if self.bytes_per_second > 0.0 {
                    self.bytes_per_second * 0.6 + current * 0.4
                } else {
                    current
                };
            }
            _ => self.bytes_per_second = 0.0,
        }
        self.last = Some((now, bytes));
    }
}

fn percent(value: &Value) -> Option<f32> {
    value["progress"]
        .as_f64()
        .map(|progress| (progress / 100.0).clamp(0.0, 1.0) as f32)
}

fn text(value: &Value, field: &str, fallback: &str) -> String {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

impl NativeApp {
    pub(crate) fn jobs(&self) -> Vec<Job> {
        let mut jobs = Vec::new();
        let status = self.render["status"].as_str().unwrap_or_default();
        if matches!(status, "preparing" | "running" | "paused" | "stopping") {
            let frame = self.render["current_frame"].as_u64().unwrap_or(0);
            let fraction = self
                .render_total
                .filter(|_| frame > 0)
                .map(|total| (frame as f64 / total).min(1.0) as f32);
            let mut facts = Vec::new();
            if frame > 0 {
                facts.push(match self.render_total {
                    Some(total) => format!("frame {frame} of ≈{total:.0}"),
                    None => format!("frame {frame}"),
                });
            }
            if let Some(fps) = self.render["gen_fps"].as_f64() {
                facts.push(format!("{fps:.1} fps"));
            }
            jobs.push(Job {
                activity: Activity {
                    id: "render",
                    icon: "play",
                    title: match status {
                        "preparing" => "Preparing render",
                        "paused" => "Render paused",
                        "stopping" => "Stopping render",
                        _ => "Rendering video",
                    }
                    .to_owned(),
                    detail: String::new(),
                    fraction,
                    facts,
                },
                eta: self.render["eta"].as_str().map(str::to_owned),
                forecast: status == "running",
                page: PROCESS_PAGE,
                cancel: Some("/api/render/stop"),
            });
        }
        if self.download["active"].as_bool() == Some(true) {
            let received = self.download["bytes_received"].as_u64().unwrap_or(0);
            let total = self.download["bytes_total"].as_u64().unwrap_or(0);
            let mut facts = Vec::new();
            if total > 0 {
                facts.push(format!(
                    "{} of {}",
                    format_bytes(received),
                    format_bytes(total)
                ));
            }
            if self.download_rate.bytes_per_second > 0.0 {
                facts.push(format!(
                    "{}/s",
                    format_bytes(self.download_rate.bytes_per_second as u64)
                ));
            }
            jobs.push(Job {
                activity: Activity {
                    id: "download",
                    icon: "download",
                    title: text(&self.download, "label", "Downloading models"),
                    detail: String::new(),
                    fraction: percent(&self.download),
                    facts,
                },
                eta: None,
                forecast: true,
                page: MODELS_PAGE,
                cancel: Some("/api/actions/cancel"),
            });
        }
        if self.install["status"] == "running" {
            jobs.push(Job {
                activity: Activity {
                    id: "runtime",
                    icon: "wrench",
                    title: text(&self.install, "stage", "Runtime setup"),
                    detail: text(&self.install, "detail", ""),
                    fraction: percent(&self.install),
                    facts: Vec::new(),
                },
                eta: None,
                forecast: false,
                page: RUNTIME_PAGE,
                cancel: Some("/api/actions/cancel"),
            });
        }
        if self.benchmarks["status"] == "running" {
            let done = self.benchmarks["completed"].as_u64().unwrap_or(0);
            let total = self.benchmarks["total"].as_u64().unwrap_or(0);
            jobs.push(Job {
                activity: Activity {
                    id: "benchmark",
                    icon: "benchmark",
                    title: text(&self.benchmarks, "stage", "Benchmarking models"),
                    detail: text(&self.benchmarks, "detail", ""),
                    fraction: percent(&self.benchmarks),
                    facts: if total > 0 {
                        vec![format!("{done} of {total} tests")]
                    } else {
                        Vec::new()
                    },
                },
                eta: None,
                forecast: true,
                page: BENCHMARK_PAGE,
                cancel: Some("/api/benchmarks/cancel"),
            });
        }
        if matches!(
            self.storage_status["status"].as_str(),
            Some("starting" | "installing")
        ) {
            jobs.push(Job {
                activity: Activity {
                    id: "storage",
                    icon: "storage",
                    title: text(&self.storage_status, "stage", "Preparing model storage"),
                    detail: text(&self.storage_status, "detail", ""),
                    fraction: percent(&self.storage_status),
                    facts: Vec::new(),
                },
                eta: None,
                forecast: true,
                page: SETTINGS_PAGE,
                cancel: Some("/api/actions/cancel"),
            });
        }
        if self.update["status"] == "downloading" {
            jobs.push(Job {
                activity: Activity {
                    id: "update",
                    icon: "update",
                    title: "Downloading app update".to_owned(),
                    detail: String::new(),
                    fraction: percent(&self.update),
                    facts: Vec::new(),
                },
                eta: None,
                forecast: true,
                page: SETTINGS_PAGE,
                cancel: Some("/api/actions/cancel"),
            });
        }
        let waiting = [
            (
                self.footprint_loading,
                "footprint",
                "storage",
                "Measuring disk usage",
                SETTINGS_PAGE,
            ),
            (
                self.library_loading,
                "catalogue",
                "library",
                "Loading the community catalogue",
                COMMUNITY_PAGE,
            ),
        ];
        for (active, id, icon, title, page) in waiting {
            if active {
                jobs.push(Job {
                    activity: Activity {
                        id,
                        icon,
                        title: title.to_owned(),
                        detail: String::new(),
                        fraction: None,
                        facts: Vec::new(),
                    },
                    eta: None,
                    forecast: false,
                    page,
                    cancel: None,
                });
            }
        }
        jobs
    }

    /// Update elapsed times once per frame, whichever page is showing.
    pub(crate) fn track_jobs(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let jobs = self.jobs();
        let ids = jobs.iter().map(|job| job.activity.id).collect::<Vec<_>>();
        self.eta.retain(&ids);
        self.timings = jobs
            .iter()
            .map(|job| {
                let timing =
                    self.eta
                        .update(job.activity.id, job.activity.fraction.unwrap_or(0.0), now);
                (job.activity.id, timing)
            })
            .collect();
        if !jobs.is_empty() {
            // Keeps the elapsed clock ticking between engine updates.
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    pub(crate) fn job_timing(&self, job: &Job) -> Timing {
        let mut timing = self
            .timings
            .get(job.activity.id)
            .copied()
            .unwrap_or(Timing {
                elapsed: Duration::ZERO,
                remaining: None,
            });
        if !job.forecast {
            timing.remaining = None;
        }
        timing
    }

    pub(crate) fn job_block(&mut self, ui: &mut egui::Ui, job: &Job, with_view: bool) {
        let palette = self.palette();
        let timing = self.job_timing(job);
        let eta = job.eta.as_deref();
        let mut view = false;
        let mut cancel = false;
        progress::block(
            ui,
            &job.activity,
            timing,
            palette.accent,
            eta,
            job.forecast,
            |ui| {
                if job.cancel.is_some() {
                    cancel = components::icon_button(ui, "close", "Cancel", true).clicked();
                }
                if with_view {
                    view = components::icon_button(ui, "eye", "View", true).clicked();
                }
            },
        );
        if view {
            self.page = job.page;
        }
        if cancel && let Some(endpoint) = job.cancel {
            if job.activity.id == "render" {
                self.queue.stop("Queue stopped.");
            }
            self.post(endpoint, json!({}));
        }
    }

    /// Jobs that the current page does not already show for itself.
    pub(crate) fn activity_strip(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        for job in self.jobs() {
            let shown_by_page = (job.activity.id == "render" && self.page == PROCESS_PAGE)
                || (job.activity.id == "download" && self.page == MODELS_PAGE);
            if shown_by_page {
                continue;
            }
            let here = job.page == self.page;
            components::glass_frame(palette)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    self.job_block(ui, &job, !here);
                });
            ui.add_space(theme::space::SM);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rate;
    use std::time::{Duration, Instant};

    #[test]
    fn rate_is_measured_between_samples_and_resets_for_a_new_file() {
        let mut rate = Rate::default();
        let start = Instant::now();
        rate.sample(0, start);
        rate.sample(2_000_000, start + Duration::from_secs(2));
        assert_eq!(rate.bytes_per_second, 1_000_000.0);
        rate.sample(10, start + Duration::from_secs(3));
        assert_eq!(rate.bytes_per_second, 0.0);
    }
}
