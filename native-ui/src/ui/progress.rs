//! Progress with an estimated time remaining.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, Rect, RichText, Sense, Vec2};

use super::{components, theme};

/// Estimates need a little history before they are worth showing.
const MIN_ELAPSED: Duration = Duration::from_secs(3);
const MIN_ADVANCE: f32 = 0.01;

#[derive(Clone, Copy)]
struct Track {
    started: Instant,
    first: f32,
    last: f32,
}

#[derive(Default)]
pub struct EtaTracker {
    tracks: HashMap<&'static str, Track>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub elapsed: Duration,
    pub remaining: Option<Duration>,
}

impl EtaTracker {
    /// Record where a task is now and estimate what is left.
    pub fn update(&mut self, id: &'static str, fraction: f32, now: Instant) -> Timing {
        let fraction = fraction.clamp(0.0, 1.0);
        let track = self.tracks.entry(id).or_insert(Track {
            started: now,
            first: fraction,
            last: fraction,
        });
        // Going backwards means a new run of the same kind of task.
        if fraction + 0.001 < track.last {
            *track = Track {
                started: now,
                first: fraction,
                last: fraction,
            };
        }
        track.last = fraction;
        let elapsed = now.duration_since(track.started);
        let advanced = fraction - track.first;
        let remaining = (elapsed >= MIN_ELAPSED && advanced >= MIN_ADVANCE && fraction < 1.0)
            .then(|| elapsed.mul_f32((1.0 - fraction) / advanced));
        Timing { elapsed, remaining }
    }

    /// Forget tasks that are no longer running.
    pub fn retain(&mut self, active: &[&'static str]) {
        self.tracks.retain(|id, _| active.contains(id));
    }
}

pub fn short_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!("{} h {:02} min", seconds / 3600, seconds / 60 % 60)
    } else if seconds >= 60 {
        format!("{} min {:02} s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds} s")
    }
}

/// One running job, ready to draw.
pub struct Activity {
    pub id: &'static str,
    pub icon: &'static str,
    pub title: String,
    pub detail: String,
    /// `None` when the job cannot say how far along it is.
    pub fraction: Option<f32>,
    /// Extra facts such as speed or frame rate.
    pub facts: Vec<String>,
}

/// The bar itself: eased fill, or a sliding segment when the length is unknown.
pub fn bar(ui: &mut egui::Ui, id: &str, fraction: Option<f32>, height: f32) {
    match fraction {
        Some(fraction) => components::progress_bar(ui, id, fraction, height),
        None => {
            let palette = components::palette_of(ui);
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width().max(1.0), height),
                Sense::hover(),
            );
            ui.painter()
                .rect_filled(rect, height / 2.0, palette.elevated);
            if ui.is_rect_visible(rect) {
                let time = ui.input(|input| input.time) as f32;
                let segment = rect.width() * 0.28;
                let travel = rect.width() + segment;
                let left = rect.left() - segment + (time * 0.55).fract() * travel;
                let moving = Rect::from_min_max(
                    egui::pos2(left.max(rect.left()), rect.top()),
                    egui::pos2((left + segment).min(rect.right()), rect.bottom()),
                );
                components::gradient_rect(
                    ui.painter(),
                    moving,
                    height / 2.0,
                    palette.accent,
                    palette.accent_alt,
                );
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
        }
    }
}

pub fn timing_text(
    timing: Timing,
    fraction: Option<f32>,
    engine_eta: Option<&str>,
    forecast: bool,
) -> String {
    let mut parts = vec![format!("{} elapsed", short_duration(timing.elapsed))];
    match engine_eta.filter(|eta| !eta.is_empty() && *eta != "--") {
        Some(eta) => parts.push(format!("ETA {eta}")),
        None => match timing.remaining {
            Some(remaining) => parts.push(format!("about {} left", short_duration(remaining))),
            None if forecast && fraction.is_some_and(|value| value < 1.0) => {
                parts.push("estimating time left…".to_owned());
            }
            None => {}
        },
    }
    parts.join("  ·  ")
}

/// A full progress block: icon, title, percentage, bar, and timing.
pub fn block(
    ui: &mut egui::Ui,
    activity: &Activity,
    timing: Timing,
    tone: Color32,
    engine_eta: Option<&str>,
    forecast: bool,
    trailing: impl FnOnce(&mut egui::Ui),
) {
    let palette = components::palette_of(ui);
    ui.horizontal(|ui| {
        components::icon_tile(ui, activity.icon, tone, 36.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            trailing(ui);
            if let Some(fraction) = activity.fraction {
                ui.label(
                    RichText::new(format!("{:.0}%", fraction * 100.0))
                        .font(theme::title_font(20.0))
                        .color(palette.text),
                );
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.add(
                        egui::Label::new(
                            RichText::new(&activity.title)
                                .font(theme::title_font(13.5))
                                .color(palette.text),
                        )
                        .truncate(),
                    );
                    let mut line =
                        vec![timing_text(timing, activity.fraction, engine_eta, forecast)];
                    line.extend(activity.facts.iter().cloned());
                    if !activity.detail.is_empty() {
                        line.push(activity.detail.clone());
                    }
                    let line = line.join("  ·  ");
                    ui.add(
                        egui::Label::new(RichText::new(&line).size(11.5).color(palette.secondary))
                            .truncate(),
                    )
                    .on_hover_text(&line);
                });
            });
        });
    });
    bar(ui, activity.id, activity.fraction, 7.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_follows_the_observed_rate() {
        let mut tracker = EtaTracker::default();
        let start = Instant::now();
        assert_eq!(tracker.update("a", 0.0, start).remaining, None);
        let timing = tracker.update("a", 0.25, start + Duration::from_secs(10));
        assert_eq!(timing.remaining, Some(Duration::from_secs(30)));
        assert_eq!(timing.elapsed, Duration::from_secs(10));
    }

    #[test]
    fn eta_is_withheld_early_and_restarts_when_progress_goes_back() {
        let mut tracker = EtaTracker::default();
        let start = Instant::now();
        tracker.update("a", 0.0, start);
        assert_eq!(
            tracker
                .update("a", 0.5, start + Duration::from_secs(1))
                .remaining,
            None
        );
        let again = tracker.update("a", 0.1, start + Duration::from_secs(20));
        assert_eq!(again.elapsed, Duration::ZERO);
        tracker.retain(&[]);
        assert_eq!(
            tracker
                .update("a", 0.9, start + Duration::from_secs(30))
                .elapsed,
            Duration::ZERO
        );
    }

    #[test]
    fn uneven_jobs_show_elapsed_time_without_promising_an_end() {
        let timing = Timing {
            elapsed: Duration::from_secs(40),
            remaining: None,
        };
        assert_eq!(timing_text(timing, Some(0.5), None, false), "40 s elapsed");
        assert!(timing_text(timing, Some(0.5), None, true).contains("estimating"));
        assert!(timing_text(timing, Some(0.5), Some("00:12"), true).contains("ETA 00:12"));
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(short_duration(Duration::from_secs(9)), "9 s");
        assert_eq!(short_duration(Duration::from_secs(130)), "2 min 10 s");
        assert_eq!(short_duration(Duration::from_secs(3720)), "1 h 02 min");
    }
}
