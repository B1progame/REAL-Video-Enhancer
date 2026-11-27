//! Small illustrations drawn with the painter, so they cost no image memory,
//! stay sharp at any size, and follow the accent colour.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};

use super::components::{self, gradient_rect};

fn at(rect: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(
        rect.left() + rect.width() * x,
        rect.top() + rect.height() * y,
    )
}

/// A cheap repeatable pseudo-random value in 0..1.
fn noise(index: u32) -> f32 {
    let mixed = index.wrapping_mul(2_654_435_761).rotate_left(13) ^ 0x9E37_79B9;
    (mixed.wrapping_mul(1_103_515_245) >> 16) as f32 / 65_535.0
}

fn mountains(painter: &egui::Painter, rect: Rect, color: Color32) {
    for (left, peak, right, height, alpha) in [
        (0.50, 0.68, 0.90, 0.30, 0.95),
        (0.66, 0.84, 1.02, 0.50, 0.6),
    ] {
        painter.add(Shape::convex_polygon(
            vec![
                at(rect, left, 1.0),
                at(rect, peak, height),
                at(rect, right, 1.0),
            ],
            color.gamma_multiply(alpha),
            Stroke::NONE,
        ));
    }
    painter.circle_filled(at(rect, 0.88, 0.24), rect.height() * 0.10, color);
}

/// Draw the picture that explains what one kind of model does.
pub fn category(painter: &egui::Painter, rect: Rect, kind: &str, color: Color32) {
    let painter = painter.with_clip_rect(rect);
    let height = rect.height();
    match kind {
        // Coarse pixels on the left turn into a clean picture on the right.
        "upscale" => {
            let cell = height / 5.0;
            for column in 0..((rect.width() * 0.46 / cell) as u32) {
                for row in 0..5 {
                    let strength = noise(column * 7 + row) * (column as f32 + 1.0) / 14.0;
                    if row as f32 >= 4.0 - column as f32 * 0.5 {
                        painter.rect_filled(
                            Rect::from_min_size(
                                rect.min + Vec2::new(column as f32 * cell, row as f32 * cell),
                                Vec2::splat(cell - 1.0),
                            ),
                            1.0,
                            color.gamma_multiply((strength + 0.12).min(0.8)),
                        );
                    }
                }
            }
            painter.line_segment(
                [at(rect, 0.48, 0.05), at(rect, 0.48, 0.95)],
                Stroke::new(1.5, color),
            );
            mountains(&painter, rect, color);
        }
        // One object caught at several moments.
        "interpolate" => {
            for step in 0..7 {
                let t = step as f32 / 6.0;
                let y = 0.72 - (t * std::f32::consts::PI).sin() * 0.45;
                let full = step == 6 || step == 0;
                painter.circle_filled(
                    at(rect, 0.12 + t * 0.76, y),
                    height * if full { 0.15 } else { 0.11 },
                    color.gamma_multiply(if full { 1.0 } else { 0.18 + t * 0.35 }),
                );
            }
        }
        // Grain that thins out into calm.
        "denoise" => {
            for index in 0..110 {
                let x = noise(index);
                let y = noise(index + 500);
                if noise(index + 900) > x {
                    painter.circle_filled(at(rect, x, y), 1.4, color.gamma_multiply(0.85));
                }
            }
            painter.add(Shape::line(
                (0..=30)
                    .map(|i| {
                        let t = i as f32 / 30.0;
                        let jitter = (noise(i + 40) - 0.5) * (1.0 - t) * 0.7;
                        at(rect, t, 0.5 + (t * 6.0).sin() * 0.2 + jitter)
                    })
                    .collect(),
                Stroke::new(1.8, color),
            ));
        }
        // Compression blocks dissolving into a smooth ramp.
        "decompress" => {
            let cell = height / 3.0;
            for column in 0..((rect.width() * 0.5 / cell) as u32) {
                for row in 0..3 {
                    painter.rect_filled(
                        Rect::from_min_size(
                            rect.min + Vec2::new(column as f32 * cell, row as f32 * cell),
                            Vec2::splat(cell),
                        ),
                        0.0,
                        color.gamma_multiply(0.15 + noise(column * 3 + row + 77) * 0.55),
                    );
                }
            }
            gradient_rect(
                &painter,
                Rect::from_min_max(at(rect, 0.52, 0.12), at(rect, 0.96, 0.88)),
                6.0,
                color.gamma_multiply(0.2),
                color,
            );
        }
        // Soft rings sharpening into a crisp one.
        "deblur" => {
            for ring in 0..4 {
                let t = ring as f32 / 3.0;
                painter.circle_stroke(
                    at(rect, 0.18 + t * 0.62, 0.5),
                    height * 0.30,
                    Stroke::new(9.0 - t * 7.0, color.gamma_multiply(0.18 + t * 0.82)),
                );
            }
        }
        // Film frames with a cut between them.
        _ => {
            for (index, x) in [0.06_f32, 0.30, 0.58, 0.82].into_iter().enumerate() {
                let frame = Rect::from_min_max(at(rect, x, 0.2), at(rect, x + 0.17, 0.8));
                painter.rect_filled(
                    frame,
                    3.0,
                    color.gamma_multiply(if index < 2 { 0.35 } else { 0.8 }),
                );
            }
            painter.line_segment(
                [at(rect, 0.52, 0.04), at(rect, 0.52, 0.96)],
                Stroke::new(2.0, color),
            );
        }
    }
}

/// A wide picture strip with a soft backdrop, used on cards.
pub fn banner(ui: &mut egui::Ui, kind: &str, color: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(1.0), height),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let corner = f32::from(super::theme::control_radius());
    gradient_rect(
        ui.painter(),
        rect,
        corner,
        color.gamma_multiply(0.10),
        color.gamma_multiply(0.24),
    );
    category(ui.painter(), rect.shrink(8.0), kind, color);
}

/// A miniature of the app in the current colours, for the appearance page.
pub fn theme_preview(ui: &mut egui::Ui, height: f32) {
    let palette = components::palette_of(ui);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(1.0), height),
        Sense::hover(),
    );
    let corner = f32::from(super::theme::radius());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, corner, palette.elevated);
    components::glow(
        &painter,
        at(rect, 0.15, 0.1),
        height,
        palette.accent.gamma_multiply(0.35),
    );
    components::glow(
        &painter,
        at(rect, 0.9, 0.95),
        height,
        palette.accent_alt.gamma_multiply(0.35),
    );
    let side = Rect::from_min_max(at(rect, 0.03, 0.08), at(rect, 0.22, 0.92));
    painter.rect_filled(side, corner * 0.6, palette.surface);
    for row in 0..5 {
        let item = Rect::from_min_size(
            side.min + Vec2::new(8.0, 10.0 + row as f32 * 18.0),
            Vec2::new(side.width() - 16.0, 11.0),
        );
        if row == 1 {
            gradient_rect(&painter, item, 5.0, palette.accent, palette.accent_alt);
        } else {
            painter.rect_filled(item, 5.0, palette.elevated);
        }
    }
    for (left, right) in [(0.26, 0.61), (0.64, 0.97)] {
        let card = Rect::from_min_max(at(rect, left, 0.08), at(rect, right, 0.68));
        painter.rect_filled(card, corner * 0.7, palette.surface);
        painter.rect_stroke(
            card,
            corner * 0.7,
            Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        category(
            &painter,
            card.shrink(12.0),
            if left < 0.5 { "upscale" } else { "interpolate" },
            palette.accent,
        );
    }
    let dock = Rect::from_min_max(at(rect, 0.26, 0.74), at(rect, 0.97, 0.92));
    painter.rect_filled(dock, corner * 0.6, palette.surface);
    gradient_rect(
        &painter,
        Rect::from_min_max(at(dock, 0.76, 0.2), at(dock, 0.97, 0.8)),
        corner * 0.5,
        palette.accent,
        palette.accent_alt,
    );
    painter.rect_filled(
        Rect::from_min_max(at(dock, 0.04, 0.40), at(dock, 0.55, 0.60)),
        3.0,
        palette.elevated,
    );
}

#[cfg(test)]
mod tests {
    use super::noise;

    #[test]
    fn noise_is_repeatable_and_in_range() {
        for index in 0..1000 {
            let value = noise(index);
            assert!((0.0..=1.0).contains(&value));
            assert_eq!(value, noise(index));
        }
    }
}
