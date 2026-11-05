//! One pedal, drawn large for the focused effect editor.
//!
//! The board keeps its small overview artwork. This is the stage version: the
//! same enclosure and finish, scaled up so the hardware legends and knobs are
//! legible. The knobs here are still scenery; the editor's control rack is the
//! surface a person actually turns.

use eframe::egui::{Align2, Color32, Painter, Pos2, Rect, Vec2};

use super::art::Art;
use super::fx::{Form, Pedal};
use super::hardware::{knob_angle, metal, shadow, small_knob};
use super::paint::{spaced, spaced_italic};
use super::theme::*;

/// Draw a large version of `fx` inside `area`, returning the enclosure rect.
pub(super) fn paint(painter: &Painter, art: &Art, area: Rect, fx: &Pedal, values: &[u8]) -> Rect {
    if fx.form == Form::Box {
        return boxed(painter, area, fx, values);
    }
    // The larger units are already wide (a tape echo, a reverb tank, a rack).
    // They read well at their native size, so centre one instead of redrawing
    // every profile at a second scale.
    super::pedal::paint(painter, art, area, fx, values);
    area
}

fn boxed(painter: &Painter, area: Rect, fx: &Pedal, values: &[u8]) -> Rect {
    let max_w = area.width().min(310.0);
    let max_h = area.height().min(390.0);
    let mut width = max_w.min(max_h * 0.80);
    let mut height = width * 1.14;
    if height > max_h {
        height = max_h;
        width = height / 1.14;
    }
    let unit = Rect::from_center_size(area.center(), Vec2::new(width, height));

    shadow(painter, unit, 16.0);
    metal(painter, unit, 16.0, fx.face, fx.body);
    // Brushed-metal grain, scaled with the enclosure.
    for row in 0..16 {
        let y = unit.top() + 7.0 + row as f32 * (unit.height() - 14.0) / 15.0;
        let color = if row % 2 == 0 {
            Color32::from_white_alpha(6)
        } else {
            Color32::from_black_alpha(10)
        };
        painter.line_segment(
            [
                Pos2::new(unit.left() + 8.0, y),
                Pos2::new(unit.right() - 8.0, y),
            ],
            stroke(0.7, color),
        );
    }
    painter.rect_stroke(unit, 16.0, stroke(1.2, Color32::from_black_alpha(150)));
    painter.line_segment(
        [
            unit.left_top() + Vec2::new(16.0, 1.5),
            unit.right_top() + Vec2::new(-16.0, 1.5),
        ],
        stroke(1.2, Color32::from_white_alpha(60)),
    );

    for corner in [
        unit.left_top() + Vec2::splat(15.0),
        unit.right_top() + Vec2::new(-15.0, 15.0),
        unit.left_bottom() + Vec2::new(15.0, -15.0),
        unit.right_bottom() - Vec2::splat(15.0),
    ] {
        big_screw(painter, corner);
    }

    // Side jacks, proud of the enclosure the way a real pedal's are.
    for x in [unit.left() + 1.0, unit.right() - 1.0] {
        let center = Pos2::new(x, unit.center().y + 4.0);
        painter.circle_filled(center, 10.5, fx.edge);
        painter.circle_filled(center, 7.0, Color32::from_rgb(16, 16, 18));
        painter.circle_stroke(center, 8.8, stroke(0.9, Color32::from_white_alpha(55)));
    }

    spaced(
        painter,
        unit.center_top() + Vec2::new(0.0, 12.0),
        Align2::CENTER_TOP,
        fx.brand,
        semibold(8.0),
        fx.ink.gamma_multiply(0.72),
        2.2,
    );
    spaced(
        painter,
        Pos2::new(unit.center().x, unit.top() + unit.height() * 0.58),
        Align2::CENTER_CENTER,
        fx.name,
        semibold(16.0),
        fx.ink,
        1.6,
    );
    spaced(
        painter,
        Pos2::new(unit.center().x, unit.top() + unit.height() * 0.67),
        Align2::CENTER_CENTER,
        fx.sub,
        semibold(8.5),
        fx.accent.gamma_multiply(0.95),
        1.4,
    );

    // A three-knob pedal keeps one row; a five- or six-knob model gets two,
    // so no control has to shrink into a dot.
    let count = fx.knobs.min(fx.labels.len());
    let radius = if count > 4 { 18.0 } else { 23.0 };
    let rows: Vec<(usize, usize)> = if count > 4 {
        let first = count.div_ceil(2);
        vec![(0, first), (first, count)]
    } else {
        vec![(0, count)]
    };
    for (row, (start, end)) in rows.iter().enumerate() {
        let y = if rows.len() == 1 {
            unit.top() + unit.height() * 0.33
        } else {
            unit.top() + unit.height() * (0.27 + row as f32 * 0.15)
        };
        let span = unit.width() * 0.66;
        let used = end - start;
        for slot in 0..used {
            let t = if used == 1 {
                0.5
            } else {
                slot as f32 / (used - 1) as f32
            };
            let control = Pos2::new(unit.center().x - span / 2.0 + span * t, y);
            let value = values.get(start + slot).copied().unwrap_or(0x80);
            small_knob(
                painter,
                control,
                radius,
                fx.knob,
                fx.knob_cap,
                knob_angle(value),
            );
            if let Some(label) = fx.labels.get(start + slot) {
                spaced(
                    painter,
                    control + Vec2::new(0.0, radius + 5.0),
                    Align2::CENTER_TOP,
                    label,
                    semibold(7.4),
                    fx.ink.gamma_multiply(0.82),
                    0.9,
                );
            }
        }
    }

    // The jewel and the footswitch, both enlarged.
    let jewel = unit.right_top() + Vec2::new(-25.0, 25.0);
    painter.circle_filled(jewel, 8.5, Color32::from_black_alpha(150));
    painter.circle_filled(jewel, 5.6, fx.led.gamma_multiply(0.9));
    painter.circle_filled(jewel - Vec2::new(1.2, 1.2), 2.0, Color32::WHITE);

    let button = Pos2::new(unit.center().x, unit.bottom() - 38.0);
    painter.circle_filled(
        button + Vec2::new(0.0, 2.0),
        20.0,
        Color32::from_black_alpha(130),
    );
    painter.circle_filled(button, 18.5, Color32::from_rgb(96, 98, 102));
    painter.circle_filled(button, 13.0, Color32::from_rgb(54, 55, 58));
    painter.circle_stroke(button, 18.5, stroke(0.9, Color32::from_white_alpha(65)));
    painter.circle_stroke(button, 9.0, stroke(0.8, Color32::from_black_alpha(140)));
    for turn in 0..10 {
        let angle = turn as f32 * std::f32::consts::TAU / 10.0;
        let dir = Vec2::new(angle.cos(), angle.sin());
        painter.circle_filled(button + dir * 15.6, 1.2, Color32::from_black_alpha(95));
    }

    spaced_italic(
        painter,
        unit.right_bottom() + Vec2::new(-14.0, -12.0),
        Align2::RIGHT_BOTTOM,
        "Fender",
        semibold(8.0),
        fx.ink.gamma_multiply(0.55),
        0.2,
    );

    unit
}

fn big_screw(painter: &Painter, center: Pos2) {
    painter.circle_filled(
        center + Vec2::new(0.0, 1.0),
        5.4,
        Color32::from_black_alpha(160),
    );
    painter.circle_filled(center, 4.7, Color32::from_rgb(112, 110, 106));
    painter.circle_filled(
        center - Vec2::new(0.7, 1.0),
        3.2,
        Color32::from_rgb(146, 144, 139),
    );
    let slot = Vec2::new(3.0, -1.6);
    painter.line_segment(
        [center - slot, center + slot],
        stroke(1.1, Color32::from_rgb(40, 39, 38)),
    );
}
