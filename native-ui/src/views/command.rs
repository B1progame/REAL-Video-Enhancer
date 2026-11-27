//! Ctrl+K command palette.

use eframe::egui::{self, Align2, Color32, Id, Key, Modifiers, RichText, Stroke, Vec2};
use serde_json::json;

use crate::{
    NativeApp, PAGES, PROCESS_PAGE,
    ui::{backdrop::Material, theme},
};

#[derive(Default)]
pub struct CommandPalette {
    open: bool,
    query: String,
    highlighted: usize,
    focus: bool,
}

impl CommandPalette {
    pub fn open(&mut self) {
        *self = Self {
            open: true,
            focus: true,
            ..Self::default()
        };
    }
}

#[derive(Clone, Copy)]
enum Command {
    Page(usize),
    ChooseVideo,
    ChooseOutput,
    QueueFolder,
    StartRender,
    CancelWork,
    ToggleTheme,
    Material(Material),
    Accent(usize),
    ToggleMotion,
}

fn commands() -> Vec<(String, Command)> {
    let mut list = PAGES
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (format!("Go to {name}"), Command::Page(index)))
        .collect::<Vec<_>>();
    list.extend([
        ("Choose a source video".to_owned(), Command::ChooseVideo),
        ("Choose the output folder".to_owned(), Command::ChooseOutput),
        (
            "Add a folder to the render queue".to_owned(),
            Command::QueueFolder,
        ),
        ("Start render".to_owned(), Command::StartRender),
        ("Cancel active work".to_owned(), Command::CancelWork),
        ("Switch light / dark theme".to_owned(), Command::ToggleTheme),
        ("Turn motion on or off".to_owned(), Command::ToggleMotion),
    ]);
    list.extend(Material::ALL.map(|material| {
        (
            format!("Window material: {}", material.label()),
            Command::Material(material),
        )
    }));
    list.extend(
        theme::ACCENTS
            .iter()
            .enumerate()
            .map(|(index, (name, ..))| (format!("Accent: {name}"), Command::Accent(index))),
    );
    list
}

/// Every word of the query has to appear, in any order.
fn command_matches(label: &str, query: &str) -> bool {
    let label = label.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| label.contains(word))
}

impl NativeApp {
    pub(crate) fn command_palette(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::K)) {
            if self.command.open {
                self.command.open = false;
            } else {
                self.command.open();
            }
        }
        if !self.command.open {
            return;
        }
        let matching = commands()
            .into_iter()
            .filter(|(label, _)| command_matches(label, &self.command.query))
            .collect::<Vec<_>>();
        let count = matching.len();
        let (up, down, enter, escape) = ctx.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::ArrowUp),
                input.consume_key(Modifiers::NONE, Key::ArrowDown),
                input.consume_key(Modifiers::NONE, Key::Enter),
                input.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if count > 0 {
            if down {
                self.command.highlighted = (self.command.highlighted + 1) % count;
            }
            if up {
                self.command.highlighted = (self.command.highlighted + count - 1) % count;
            }
            self.command.highlighted = self.command.highlighted.min(count - 1);
        }
        let mut chosen = enter
            .then(|| matching.get(self.command.highlighted).map(|item| item.1))
            .flatten();
        let palette = self.palette();
        let appear = ctx.animate_bool_with_time(
            Id::unique("command-appear"),
            true,
            if self.prefs.reduce_motion { 0.0 } else { 0.18 },
        );
        let modal = egui::Modal::new(Id::unique("command-palette"))
            .area(
                egui::Modal::default_area(Id::unique("command-palette"))
                    .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 84.0 + 10.0 * appear)),
            )
            .backdrop_color(Color32::from_black_alpha(120))
            .frame(
                egui::Frame::new()
                    .fill(palette.popup)
                    .stroke(Stroke::new(1.0, palette.accent.gamma_multiply(0.5)))
                    .corner_radius(theme::radius())
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ctx, |ui| {
                ui.set_width(520.0);
                let field = ui.add_sized(
                    Vec2::new(ui.available_width(), 38.0),
                    egui::TextEdit::singleline(&mut self.command.query)
                        .hint_text("Type a command…")
                        .margin(Vec2::new(12.0, 10.0)),
                );
                if self.command.focus {
                    field.request_focus();
                    self.command.focus = false;
                }
                if field.changed() {
                    self.command.highlighted = 0;
                }
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            if matching.is_empty() {
                                ui.label(
                                    RichText::new("No matching command.").color(palette.muted),
                                );
                            }
                            for (index, (label, command)) in matching.iter().enumerate() {
                                let selected = index == self.command.highlighted;
                                let response = ui.add(
                                    egui::Button::selectable(selected, label.as_str())
                                        .min_size(Vec2::new(0.0, 30.0)),
                                );
                                if selected && (up || down) {
                                    response.scroll_to_me(None);
                                }
                                if response.clicked() {
                                    chosen = Some(*command);
                                }
                            }
                        });
                    });
            });
        if escape || modal.backdrop_response.clicked() {
            self.command.open = false;
        }
        if let Some(command) = chosen {
            self.command.open = false;
            self.run_command(ctx, command);
        }
    }

    fn run_command(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::Page(page) => self.page = page,
            Command::ChooseVideo => {
                self.page = PROCESS_PAGE;
                self.get("/api/dialog/select_input");
            }
            Command::ChooseOutput => {
                self.page = PROCESS_PAGE;
                self.get("/api/dialog/select_output");
            }
            Command::QueueFolder => {
                self.page = PROCESS_PAGE;
                self.pick_queue_folder();
            }
            Command::StartRender => {
                self.page = PROCESS_PAGE;
                match self.render_blockers().first() {
                    Some(blocker) => {
                        let blocker = blocker.clone();
                        self.show_toast("Not ready to render", blocker, crate::ToastKind::Info);
                    }
                    None => self.start_render(),
                }
            }
            Command::CancelWork => {
                self.queue.stop("Queue stopped.");
                self.post("/api/actions/cancel", json!({}));
            }
            Command::ToggleTheme => {
                self.prefs.appearance = if self.prefs.appearance == "light" {
                    "dark".to_owned()
                } else {
                    "light".to_owned()
                };
                self.save_look(ctx);
            }
            Command::Material(material) => {
                self.prefs.material = material;
                self.save_look(ctx);
            }
            Command::Accent(index) => {
                let [red, green, blue] = theme::ACCENTS[index].1;
                self.prefs.accent = Color32::from_rgb(red, green, blue);
                self.save_look(ctx);
            }
            Command::ToggleMotion => {
                self.prefs.reduce_motion = !self.prefs.reduce_motion;
                self.save_look(ctx);
            }
        }
    }

    fn save_look(&mut self, ctx: &egui::Context) {
        self.apply_look(ctx);
        self.prefs.save_look();
    }
}

#[cfg(test)]
mod tests {
    use super::{command_matches, commands};

    #[test]
    fn words_match_in_any_order() {
        assert!(command_matches("Window material: Mica", "mica window"));
        assert!(!command_matches("Start render", "stop"));
        assert!(command_matches("Start render", ""));
        assert!(commands().iter().any(|(label, _)| label == "Go to Models"));
    }
}
