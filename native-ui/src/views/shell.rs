//! Window chrome: backdrop, title bar, navigation rail, page transitions,
//! dialogs, and notifications.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, Id, Layout, Pos2, Rect, ResizeDirection, RichText,
    Sense, Stroke, StrokeKind, Vec2, ViewportCommand,
};
use serde_json::json;

use crate::{
    APP_VERSION, NativeApp, PAGES, PROCESS_PAGE, ToastKind,
    ui::{backdrop, components, icons, theme},
};

const TITLEBAR_HEIGHT: f32 = 46.0;
const TOAST_SECONDS: f32 = 5.0;

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

impl NativeApp {
    pub(crate) fn palette(&self) -> theme::Palette {
        theme::palette(self.prefs.appearance == "light", self.prefs.accent)
    }

    /// Push the look preferences into egui and the Windows compositor.
    pub(crate) fn apply_look(&mut self, ctx: &egui::Context) {
        let dark = self.prefs.appearance != "light";
        self.material_active = self
            .window_handle
            .is_some_and(|window| backdrop::apply(window, self.prefs.material, dark));
        theme::set_tint_alpha(if self.material_active {
            self.prefs.material.tint_alpha(self.prefs.glass)
        } else {
            255
        });
        theme::set_radius(self.prefs.radius);
        crate::apply_theme(
            ctx,
            self.prefs.accent,
            &self.prefs.appearance,
            self.prefs.reduce_motion,
        );
        ctx.set_zoom_factor(self.prefs.zoom);
    }

    fn paint_background(&self, ui: &egui::Ui) {
        let palette = self.palette();
        let rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, palette.canvas);
        if !self.prefs.glow {
            return;
        }
        // The lights drift to a new spot when the page changes, then rest, so an
        // idle window costs nothing to keep on screen.
        let time = ui.ctx().animate_value_with_time(
            Id::unique("ambient-position"),
            self.page as f32,
            if self.prefs.reduce_motion { 0.0 } else { 0.7 },
        );
        let strength = if self.prefs.appearance == "light" {
            0.16
        } else {
            0.20
        };
        let size = rect.size();
        let wobble = |phase: f32| Vec2::new((time * 0.9 + phase).sin(), (time * 0.7 + phase).cos());
        components::glow(
            painter,
            rect.left_top() + Vec2::new(size.x * 0.18, size.y * 0.06) + wobble(0.0) * 70.0,
            size.x * 0.42,
            palette.accent.gamma_multiply(strength),
        );
        components::glow(
            painter,
            rect.right_bottom() - Vec2::new(size.x * 0.12, size.y * 0.10) + wobble(2.1) * 90.0,
            size.x * 0.46,
            palette.accent_alt.gamma_multiply(strength * 0.85),
        );
    }

    pub(crate) fn logo(&mut self, ui: &mut egui::Ui, size: f32) {
        let texture = self.logo.get_or_insert_with(|| {
            let image = crate::worker::decode_bounded(
                include_bytes!("../../../docs/assets/rve-logo.png"),
                (96, 96),
            )
            .unwrap_or_else(|_| egui::ColorImage::filled([1, 1], Color32::TRANSPARENT));
            ui.ctx()
                .load_texture("rve-logo", image, egui::TextureOptions::LINEAR)
        });
        ui.add(
            egui::Image::new((texture.id(), Vec2::splat(size)))
                .fit_to_exact_size(Vec2::splat(size))
                .corner_radius(theme::control_radius()),
        );
    }

    fn window_button(ui: &mut egui::Ui, glyph: &str) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(46.0, TITLEBAR_HEIGHT), Sense::click());
        let close = glyph == "close";
        let hover = ui.ctx().animate_bool_with_time(
            response.id,
            response.hovered(),
            ui.style().animation_time.min(theme::motion::HOVER_SECONDS),
        );
        if hover > 0.0 {
            let fill = if close {
                Color32::from_rgb(196, 54, 66)
            } else {
                components::palette_of(ui).elevated
            };
            ui.painter()
                .rect_filled(rect, 0.0, fill.gamma_multiply(hover));
        }
        let center = rect.center();
        let stroke = Stroke::new(1.1, ui.visuals().text_color().gamma_multiply(0.85));
        let painter = ui.painter();
        let line = |a: Vec2, b: Vec2| painter.line_segment([center + a, center + b], stroke);
        let square = |offset: Vec2, side: f32| {
            painter.rect_stroke(
                Rect::from_center_size(center + offset, Vec2::splat(side)),
                1.5,
                stroke,
                StrokeKind::Inside,
            );
        };
        match glyph {
            "minimize" => {
                line(Vec2::new(-5.0, 0.0), Vec2::new(5.0, 0.0));
            }
            "maximize" => square(Vec2::ZERO, 10.0),
            "restore" => {
                square(Vec2::new(-1.5, 1.5), 8.0);
                line(Vec2::new(-2.0, -4.5), Vec2::new(4.5, -4.5));
                line(Vec2::new(4.5, -4.5), Vec2::new(4.5, 2.0));
            }
            _ => {
                line(Vec2::new(-4.5, -4.5), Vec2::new(4.5, 4.5));
                line(Vec2::new(4.5, -4.5), Vec2::new(-4.5, 4.5));
            }
        }
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, glyph));
        response
    }

    fn title_bar(&mut self, root: &mut egui::Ui, ctx: &egui::Context) {
        let palette = self.palette();
        egui::Panel::top("title-bar")
            .exact_size(TITLEBAR_HEIGHT)
            .show_separator_line(false)
            .frame(egui::Frame::new())
            .show(root, |ui| {
                let bar = ui.max_rect();
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                // Registered first so the controls drawn over it win the click.
                let drag = ui.interact(bar, Id::unique("title-drag"), Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.horizontal_centered(|ui| {
                    ui.add_space(18.0);
                    self.logo(ui, 24.0);
                    ui.add_space(10.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new("REAL Video Enhancer")
                                .font(theme::title_font(13.0))
                                .color(palette.text),
                        )
                        .selectable(false),
                    );
                });

                if bar.width() > 760.0 {
                    let search = Rect::from_center_size(bar.center(), Vec2::new(300.0, 30.0));
                    let response = ui
                        .interact(search, Id::unique("title-search"), Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand);
                    let painter = ui.painter();
                    painter.rect_filled(search, 15.0, palette.elevated);
                    painter.rect_stroke(
                        search,
                        15.0,
                        Stroke::new(
                            1.0,
                            if response.hovered() {
                                palette.accent
                            } else {
                                palette.border
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        search.left_center() + Vec2::new(36.0, 0.0),
                        Align2::LEFT_CENTER,
                        "Search or run a command",
                        egui::FontId::proportional(12.5),
                        palette.muted,
                    );
                    painter.text(
                        search.right_center() - Vec2::new(14.0, 0.0),
                        Align2::RIGHT_CENTER,
                        "Ctrl K",
                        egui::FontId::proportional(11.0),
                        palette.muted,
                    );
                    ui.place(
                        Rect::from_center_size(
                            search.left_center() + Vec2::new(20.0, 0.0),
                            Vec2::splat(14.0),
                        ),
                        icons::styled("search", palette.muted, 14.0),
                    );
                    if response.clicked() {
                        self.command.open();
                    }
                }

                let buttons =
                    Rect::from_min_max(Pos2::new(bar.right() - 138.0, bar.top()), bar.max);
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(buttons)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if Self::window_button(ui, "minimize").clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                        }
                        let glyph = if maximized { "restore" } else { "maximize" };
                        if Self::window_button(ui, glyph).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                        }
                        if Self::window_button(ui, "close").clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    },
                );
            });
    }

    /// A borderless window gets no system resize border, so provide one.
    fn resize_edges(&self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let rect = ctx.content_rect();
        let Some(pointer) = ctx.input(|input| input.pointer.hover_pos()) else {
            return;
        };
        const EDGE: f32 = 6.0;
        let west = pointer.x <= rect.left() + EDGE;
        let east = pointer.x >= rect.right() - EDGE;
        let north = pointer.y <= rect.top() + EDGE;
        let south = pointer.y >= rect.bottom() - EDGE;
        let (direction, cursor) = match (north, south, west, east) {
            (true, _, true, _) => (ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
            (true, _, _, true) => (ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
            (_, true, true, _) => (ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
            (_, true, _, true) => (ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
            (true, ..) => (ResizeDirection::North, CursorIcon::ResizeVertical),
            (_, true, ..) => (ResizeDirection::South, CursorIcon::ResizeVertical),
            (_, _, true, _) => (ResizeDirection::West, CursorIcon::ResizeHorizontal),
            (_, _, _, true) => (ResizeDirection::East, CursorIcon::ResizeHorizontal),
            _ => return,
        };
        ctx.set_cursor_icon(cursor);
        if ctx.input(|input| input.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }

    fn navigation(&mut self, root: &mut egui::Ui) {
        let palette = self.palette();
        let compact = root.available_width() < 980.0;
        let width = if compact { 78.0 } else { 224.0 };
        egui::Panel::left("navigation")
            .resizable(false)
            .exact_size(width)
            .show_separator_line(false)
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: 12,
                right: 2,
                top: 2,
                bottom: 12,
            }))
            .show(root, |ui| {
                components::glass_frame(palette)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        self.navigation_items(ui, compact);
                    });
            });
    }

    fn navigation_items(&mut self, ui: &mut egui::Ui, compact: bool) {
        let palette = self.palette();
        const ROW: f32 = 40.0;
        const GAP: f32 = 4.0;
        const GROUP: f32 = 22.0;
        let origin = ui.cursor().min;
        let width = ui.available_width();
        let groups = [(0, "CREATE"), (2, "MODELS"), (4, "ENGINE"), (7, "APP")];
        let row_top = |index: usize| {
            let headings = groups.iter().filter(|(first, _)| *first <= index).count();
            origin.y + index as f32 * (ROW + GAP) + headings as f32 * GROUP
        };
        let indicator = ui.ctx().animate_value_with_time(
            Id::unique("navigation-indicator"),
            row_top(self.page) - origin.y,
            ui.style().animation_time,
        );
        let pill = Rect::from_min_size(
            Pos2::new(origin.x, origin.y + indicator),
            Vec2::new(width, ROW),
        );
        components::gradient_rect(
            ui.painter(),
            pill,
            f32::from(theme::control_radius()),
            palette.accent.gamma_multiply(0.34),
            palette.accent_alt.gamma_multiply(0.14),
        );
        ui.painter().rect_filled(
            Rect::from_center_size(
                pill.left_center() + Vec2::new(2.0, 0.0),
                Vec2::new(3.0, 18.0),
            ),
            2.0,
            palette.accent,
        );

        for (index, (label, icon)) in PAGES.iter().enumerate() {
            let rect =
                Rect::from_min_size(Pos2::new(origin.x, row_top(index)), Vec2::new(width, ROW));
            if let Some((_, heading)) = groups.iter().find(|(first, _)| *first == index)
                && !compact
            {
                ui.painter().text(
                    rect.left_top() + Vec2::new(10.0, -7.0),
                    Align2::LEFT_BOTTOM,
                    heading,
                    theme::title_font(10.0),
                    palette.muted,
                );
            }
            let response = ui
                .interact(rect, Id::unique(("navigation", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.page == index;
            let hover = ui.ctx().animate_bool_with_time(
                Id::unique(("navigation-hover", index)),
                response.hovered() && !selected,
                ui.style().animation_time.min(theme::motion::HOVER_SECONDS),
            );
            if hover > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    f32::from(theme::control_radius()),
                    palette.elevated.gamma_multiply(hover),
                );
            }
            let color = if selected {
                palette.text
            } else {
                palette.secondary
            };
            let icon_center = if compact {
                rect.center()
            } else {
                Pos2::new(rect.left() + 23.0 + hover * 2.0, rect.center().y)
            };
            ui.place(
                Rect::from_center_size(icon_center, Vec2::splat(18.0)),
                icons::styled(icon, if selected { palette.accent } else { color }, 18.0),
            );
            if !compact {
                ui.painter().text(
                    Pos2::new(rect.left() + 44.0 + hover * 2.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    *label,
                    if selected {
                        theme::title_font(13.5)
                    } else {
                        egui::FontId::proportional(13.5)
                    },
                    color,
                );
            }
            if index == PROCESS_PAGE && !self.queue.items.is_empty() {
                let badge = Pos2::new(rect.right() - 14.0, rect.center().y);
                ui.painter().circle_filled(badge, 9.0, palette.accent);
                ui.painter().text(
                    badge,
                    Align2::CENTER_CENTER,
                    self.queue.items.len().min(99).to_string(),
                    theme::title_font(10.0),
                    theme::contrast_text(palette.accent),
                );
            }
            if response.has_focus() {
                ui.painter().rect_stroke(
                    rect,
                    f32::from(theme::control_radius()),
                    Stroke::new(1.0, palette.accent),
                    StrokeKind::Inside,
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, true, selected, *label)
            });
            if response
                .on_hover_text(format!("{label} · Ctrl+{}", index + 1))
                .clicked()
            {
                self.page = index;
            }
        }

        let footer = ui.max_rect();
        let (state, color) = if self.connected {
            ("Engine ready", palette.success)
        } else if self.engine_error.is_some() {
            ("Engine unavailable", palette.error)
        } else {
            ("Starting engine", palette.warning)
        };
        // Only draw the footer when it cannot collide with the last item.
        if footer.bottom() - 40.0 > row_top(PAGES.len() - 1) + ROW {
            let dot = if compact {
                Pos2::new(footer.center().x, footer.bottom() - 16.0)
            } else {
                Pos2::new(footer.left() + 14.0, footer.bottom() - 26.0)
            };
            ui.painter()
                .circle_filled(dot, 7.0, color.gamma_multiply(0.25));
            ui.painter().circle_filled(dot, 3.5, color);
            if !compact {
                ui.painter().text(
                    dot + Vec2::new(14.0, 0.0),
                    Align2::LEFT_CENTER,
                    state,
                    egui::FontId::proportional(12.5),
                    palette.text,
                );
                ui.painter().text(
                    dot + Vec2::new(14.0, 17.0),
                    Align2::LEFT_CENTER,
                    format!("v{APP_VERSION}"),
                    egui::FontId::proportional(10.5),
                    palette.muted,
                );
            }
        }
    }

    fn page_content(&mut self, root: &mut egui::Ui, ctx: &egui::Context) {
        if self.shown_page != self.page {
            // Free page-specific textures and data as soon as the page is left.
            self.release_page_memory(self.shown_page);
            self.shown_page = self.page;
            self.page_changed = Instant::now();
        }
        let duration = if self.prefs.reduce_motion {
            0.0
        } else {
            theme::motion::PAGE_SECONDS
        };
        let progress = if duration == 0.0 {
            1.0
        } else {
            ease_out(self.page_changed.elapsed().as_secs_f32() / duration)
        };
        if progress < 1.0 {
            ctx.request_repaint();
        }

        if self.page == PROCESS_PAGE {
            let palette = self.palette();
            egui::Panel::bottom("render-dock")
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::new().inner_margin(egui::Margin {
                    left: 14,
                    right: 14,
                    top: 4,
                    bottom: 12,
                }))
                .show(root, |ui| {
                    components::glass_frame(palette)
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .show(ui, |ui| self.render_dock(ui));
                });
        }

        let side = if root.available_width() < 900.0 {
            14
        } else {
            22
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin {
                left: side,
                right: side,
                top: 6,
                bottom: 8,
            }))
            .show(root, |ui| {
                ui.set_opacity(0.15 + 0.85 * progress);
                ui.add_space((1.0 - progress) * 16.0);
                if self.quit_when_idle {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            self.palette().warning,
                            "Will quit after current work finishes.",
                        );
                        if ui.small_button("Cancel exit").clicked() {
                            self.quit_when_idle = false;
                        }
                    });
                }
                self.header(ui);
                self.activity_strip(ui);
                egui::ScrollArea::vertical()
                    .id_salt(("page-scroll", self.page))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.page {
                            1 => self.studio_page(ui),
                            2 => self.models_page(ui),
                            3 => self.model_library_page(ui),
                            4 => self.benchmarks_page(ui),
                            5 => self.runtime_page(ui),
                            6 => self.python_page(ui),
                            7 => self.settings_page(ui),
                            _ => self.home_page(ui),
                        }
                        ui.add_space(theme::space::MD);
                    });
            });
    }

    fn release_page_memory(&mut self, left: usize) {
        match left {
            2 => self.model_browser.release(),
            3 => {
                self.library = serde_json::Value::Null;
                self.library_loading = false;
                self.library_images.clear();
                self.library_image_order.clear();
                self.library_image_failed.clear();
            }
            _ => {}
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let (title, icon) = PAGES.get(self.page).copied().unwrap_or(PAGES[0]);
        ui.horizontal(|ui| {
            components::icon_tile(ui, icon, palette.accent, 44.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(
                    RichText::new(title)
                        .font(theme::title_font(theme::type_scale::PAGE - 4.0))
                        .color(palette.text),
                );
                ui.label(
                    RichText::new(self.page_description())
                        .size(12.5)
                        .color(palette.secondary),
                );
            });
            if ui.available_width() > 190.0 {
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
            }
        });
        ui.add_space(theme::space::MD);
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.close_confirmation {
            return;
        }
        let active = self.active_task().unwrap_or("A background task");
        egui::Modal::new(Id::unique("close-dialog")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.label(RichText::new("Work is still running").font(theme::title_font(17.0)));
            ui.add_space(theme::space::SM);
            ui.label(format!(
                "{active} is still in progress. Keep REAL Video Enhancer in the system tray? You can reopen it there while the work continues."
            ));
            if let Some(error) = &self.tray_error {
                ui.colored_label(self.palette().error, error);
            }
            ui.add_space(theme::space::MD);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(self.tray_available(), egui::Button::new("Keep working in tray"))
                    .clicked()
                {
                    self.close_confirmation = false;
                    self.quit_when_idle = false;
                    self.hide_to_tray(ctx);
                }
                if ui.button("Keep app open").clicked() {
                    self.close_confirmation = false;
                    self.quit_when_idle = false;
                }
                if ui.button("Cancel work and quit").clicked() {
                    self.close_confirmation = false;
                    self.quit_when_idle = true;
                    self.queue.stop("Queue stopped.");
                    self.post("/api/actions/cancel", json!({}));
                }
            });
        });
    }

    fn destructive_dialog(&mut self, ctx: &egui::Context) {
        let Some(kind) = self.destructive_confirmation.clone() else {
            return;
        };
        let (title, detail, action) = match kind.as_str() {
            "python" => (
                "Delete managed Python?",
                "This removes the app-managed Python runtime and its installed packages. Runtime setup can install them again.",
                "Delete files",
            ),
            "uv_cache" => (
                "Prune unused package cache?",
                "uv will prune package cache entries it considers unused. Your installed Python runtime and packages stay in place. Removed packages may need to be downloaded again.",
                "Prune cache",
            ),
            "slim" => (
                "Slim the Python runtime?",
                "This deletes the C++ headers and static libraries shipped inside PyTorch. They are only used to compile extensions; rendering does not need them. Reinstalling PyTorch brings them back.",
                "Slim runtime",
            ),
            _ => (
                "Delete all managed models?",
                "This removes downloaded model weights, including custom models, from the selected Windows or WSL storage location. You can download them again later.",
                "Delete files",
            ),
        };
        let allowed = self.active_task().is_none() && !self.managed_data_pending;
        egui::Modal::new(Id::unique("destructive-dialog")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.label(RichText::new(title).font(theme::title_font(17.0)));
            ui.add_space(theme::space::SM);
            ui.label(detail);
            ui.add_space(theme::space::MD);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    self.destructive_confirmation = None;
                }
                if ui.add_enabled(allowed, egui::Button::new(action)).clicked() {
                    self.destructive_confirmation = None;
                    self.run_destructive(&kind);
                }
            });
        });
    }

    fn run_destructive(&mut self, kind: &str) {
        if kind == "uv_cache" || kind == "slim" {
            let (stage, endpoint) = if kind == "slim" {
                ("Slimming runtime", "/api/storage/slim")
            } else {
                ("Pruning unused package cache", "/api/storage/prune-cache")
            };
            self.install = json!({
                "status": "running", "progress": 1, "stage": stage,
                "detail": "Starting.", "output": []
            });
            self.post(endpoint, json!({}));
            return;
        }
        self.managed_data_kind = Some(kind.to_owned());
        self.managed_data_message = Some(if kind == "python" {
            "Removing the managed Python environment, downloaded Python runtimes, and package cache…"
                .to_owned()
        } else if self.model_storage == "wsl" {
            "Removing the managed model library from WSL Ubuntu…".to_owned()
        } else {
            "Removing the managed model library from this PC…".to_owned()
        });
        self.managed_data_failed = false;
        self.managed_data_pending = true;
        self.post("/api/data/delete", json!({"kind": kind}));
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = self.toast.as_ref() else {
            return;
        };
        let now = Instant::now();
        if now >= toast.expires_at {
            self.toast = None;
            return;
        }
        let remaining = (toast.expires_at - now).as_secs_f32();
        let age = TOAST_SECONDS - remaining;
        let visible = if self.prefs.reduce_motion {
            1.0
        } else {
            ease_out(age / 0.28).min(ease_out(remaining / 0.28))
        };
        let palette = self.palette();
        let tone = match toast.kind {
            ToastKind::Info => palette.accent,
            ToastKind::Success => palette.success,
            ToastKind::Error => palette.error,
        };
        egui::Area::new(Id::unique("toast"))
            .order(egui::Order::Tooltip)
            .anchor(
                Align2::RIGHT_TOP,
                Vec2::new(-18.0 + (1.0 - visible) * 60.0, 56.0),
            )
            .show(ctx, |ui| {
                ui.set_opacity(visible);
                egui::Frame::new()
                    .fill(palette.popup)
                    .stroke(Stroke::new(1.0, tone.gamma_multiply(0.55)))
                    .corner_radius(theme::radius())
                    .inner_margin(egui::Margin::symmetric(15, 12))
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(90),
                    })
                    .show(ui, |ui| {
                        ui.set_width(320.0);
                        ui.label(
                            RichText::new(&toast.title)
                                .font(theme::title_font(13.5))
                                .color(palette.text),
                        );
                        ui.label(
                            RichText::new(&toast.detail)
                                .size(12.0)
                                .color(palette.secondary),
                        );
                        ui.add_space(4.0);
                        let (bar, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 3.0),
                            Sense::hover(),
                        );
                        let left = Rect::from_min_size(
                            bar.min,
                            Vec2::new(bar.width() * remaining / TOAST_SECONDS, bar.height()),
                        );
                        ui.painter().rect_filled(left, 2.0, tone);
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(if age < 0.3 || remaining < 0.3 {
            16
        } else {
            80
        }));
    }

    /// Highlight the whole window while files are being dragged over it.
    fn drop_overlay(&self, ctx: &egui::Context) {
        if ctx.input(|input| input.raw.hovered_files.is_empty()) {
            return;
        }
        let palette = self.palette();
        let rect = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            Id::unique("drop-overlay"),
        ));
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(150));
        painter.rect_stroke(
            rect.shrink(18.0),
            f32::from(theme::radius()),
            Stroke::new(2.0, palette.accent),
            StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Drop to add · extra files join the render queue",
            theme::title_font(20.0),
            Color32::WHITE,
        );
    }

    pub(crate) fn draw_shell(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        if let Some(window) = self.window_handle {
            backdrop::hide_system_buttons(window);
        }
        self.paint_background(root);
        #[cfg(windows)]
        self.title_bar(root, &ctx);
        self.navigation(root);
        self.page_content(root, &ctx);
        self.close_dialog(&ctx);
        self.destructive_dialog(&ctx);
        self.model_delete_dialog(&ctx);
        self.command_palette(&ctx);
        self.drop_overlay(&ctx);
        self.draw_toast(&ctx);
        #[cfg(windows)]
        self.resize_edges(&ctx);
    }
}
