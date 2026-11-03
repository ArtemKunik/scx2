//! The larger effect units: tape echo, spring reverb tank, rack processor.

use eframe::egui::{Align2, Color32, Painter, Pos2, Rect, Vec2};

use super::ampstyle::Era;
use super::art::Art;
use super::fx::Pedal;
use super::hardware::*;
use super::paint::{screw, spaced, spaced_italic, tile};
use super::theme::*;

pub(super) fn tape(painter: &Painter, art: &Art, unit: Rect, fx: &Pedal) {
    let radius = 7.0;
    shadow(painter, unit, radius);
    metal(painter, unit, radius, fx.body, fx.edge);
    brushed(painter, unit);
    painter.rect_stroke(unit, radius, stroke(1.0, Color32::from_black_alpha(130)));
    let window = Rect::from_min_max(
        unit.left_top() + Vec2::new(10.0, 8.0),
        unit.right_top() + Vec2::new(-10.0, 56.0),
    );
    painter.rect_filled(window, 4.0, Color32::from_rgb(22, 21, 18));
    painter.rect_stroke(window, 4.0, stroke(1.4, Color32::from_rgb(142, 136, 118)));
    reels(painter, window, fx.body);
    spaced(
        painter,
        Pos2::new(unit.center().x, window.bottom() + 7.0),
        Align2::CENTER_TOP,
        fx.name,
        semibold(8.6),
        fx.ink,
        1.2,
    );
    spaced(
        painter,
        Pos2::new(unit.center().x, unit.bottom() - 10.0),
        Align2::CENTER_BOTTOM,
        fx.brand,
        semibold(5.6),
        fx.ink.gamma_multiply(0.7),
        1.4,
    );
    knob_row(painter, fx, unit, unit.bottom() - 34.0, 8.0);
    led(
        painter,
        art,
        unit.right_top() + Vec2::new(-16.0, 16.0),
        fx.led,
        true,
    );
    for corner in [
        unit.left_top() + Vec2::new(7.0, 7.0),
        unit.right_top() + Vec2::new(-7.0, 7.0),
        unit.left_bottom() + Vec2::new(7.0, -7.0),
        unit.right_bottom() + Vec2::new(-7.0, -7.0),
    ] {
        screw(painter, corner);
    }
}

pub(super) fn tank(painter: &Painter, art: &Art, unit: Rect, fx: &Pedal) {
    let radius = 6.0;
    shadow(painter, unit, radius);
    painter.rect_filled(unit, radius, fx.body);
    tile(
        painter,
        unit,
        radius,
        &art.surface(Era::Brownface).tolex,
        art.ppp,
        Color32::WHITE,
    );
    painter.rect_stroke(unit, radius, stroke(1.0, Color32::from_black_alpha(150)));
    let handle = Rect::from_center_size(
        unit.center_top() + Vec2::new(0.0, 6.0),
        Vec2::new(78.0, 9.0),
    );
    painter.rect_filled(handle, 3.0, Color32::from_rgb(38, 26, 18));
    painter.rect_stroke(handle, 3.0, stroke(0.8, Color32::from_rgb(100, 82, 58)));
    let plate = Rect::from_min_max(
        unit.left_bottom() + Vec2::new(8.0, -52.0),
        unit.right_bottom() + Vec2::new(-8.0, -8.0),
    );
    metal(
        painter,
        plate,
        3.0,
        Color32::from_rgb(196, 198, 200),
        Color32::from_rgb(120, 122, 126),
    );
    painter.rect_stroke(plate, 3.0, stroke(1.0, Color32::from_rgb(72, 72, 74)));
    spaced_italic(
        painter,
        Pos2::new(plate.left() + 10.0, plate.center().y - 5.0),
        Align2::LEFT_CENTER,
        "Fender",
        semibold(13.0),
        fx.ink,
        0.2,
    );
    spaced(
        painter,
        Pos2::new(plate.left() + 10.0, plate.center().y + 8.0),
        Align2::LEFT_CENTER,
        fx.name,
        semibold(5.8),
        fx.ink,
        1.0,
    );
    for (index, t) in [0.56, 0.72, 0.88].into_iter().enumerate() {
        let center = Pos2::new(plate.left() + plate.width() * t, plate.center().y + 2.0);
        small_knob(
            painter,
            center,
            8.5,
            fx.knob,
            fx.knob_cap,
            -2.2 + index as f32 * 0.5,
        );
        if let Some(label) = fx.labels.get(index) {
            spaced(
                painter,
                center + Vec2::new(0.0, 11.0),
                Align2::CENTER_TOP,
                label,
                semibold(4.8),
                fx.ink,
                0.6,
            );
        }
    }
    led(
        painter,
        art,
        Pos2::new(plate.left() + plate.width() * 0.42, plate.center().y),
        fx.led,
        true,
    );
    for corner in [
        unit.left_top() + Vec2::new(7.0, 7.0),
        unit.right_top() + Vec2::new(-7.0, 7.0),
        unit.left_bottom() + Vec2::new(7.0, -7.0),
        unit.right_bottom() + Vec2::new(-7.0, -7.0),
    ] {
        screw(painter, corner);
    }
}

pub(super) fn rack(painter: &Painter, art: &Art, unit: Rect, fx: &Pedal) {
    let radius = 4.0;
    shadow(painter, unit, radius);
    painter.rect_filled(unit, radius, fx.body);
    let face = Rect::from_min_max(
        unit.left_top() + Vec2::new(10.0, 5.0),
        unit.right_bottom() + Vec2::new(-10.0, -6.0),
    );
    metal(painter, face, 2.0, fx.face, Color32::from_rgb(102, 104, 108));
    painter.rect_stroke(face, 2.0, stroke(1.0, Color32::from_rgb(58, 60, 64)));
    for x in [unit.left() + 5.0, unit.right() - 5.0] {
        screw(painter, Pos2::new(x, unit.top() + 10.0));
        screw(painter, Pos2::new(x, unit.bottom() - 10.0));
    }
    let display = Rect::from_min_size(
        face.left_top() + Vec2::new(10.0, 16.0),
        Vec2::new(72.0, 30.0),
    );
    painter.rect_filled(display, 2.0, Color32::from_rgb(10, 22, 16));
    painter.rect_stroke(display, 2.0, stroke(1.0, Color32::from_rgb(44, 66, 52)));
    spaced(
        painter,
        display.center(),
        Align2::CENTER_CENTER,
        fx.name,
        semibold(7.0),
        Color32::from_rgb(126, 224, 154),
        0.9,
    );
    spaced(
        painter,
        face.left_top() + Vec2::new(10.0, 5.0),
        Align2::LEFT_TOP,
        "FENDER",
        semibold(5.6),
        fx.ink,
        1.4,
    );
    for (index, t) in [0.64, 0.82].into_iter().enumerate() {
        let center = Pos2::new(face.left() + face.width() * t, face.center().y + 10.0);
        small_knob(
            painter,
            center,
            8.0,
            fx.knob,
            fx.knob_cap,
            -2.1 + index as f32 * 0.6,
        );
        if let Some(label) = fx.labels.get(index) {
            spaced(
                painter,
                center + Vec2::new(0.0, 10.5),
                Align2::CENTER_TOP,
                label,
                semibold(4.8),
                fx.ink,
                0.6,
            );
        }
    }
    led(
        painter,
        art,
        Pos2::new(face.left() + 10.0, face.bottom() - 10.0),
        fx.led,
        true,
    );
}
