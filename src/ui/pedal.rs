//! Choosing the footprint for an effect and drawing the stompboxes.

use eframe::egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Vec2};

use super::art::Art;
use super::fx::{Control, Form, Pedal};
use super::hardware::*;
use super::paint::{screw, spaced};
use super::theme::*;

fn stomp(painter: &Painter, art: &Art, unit: Rect, fx: &Pedal, values: &[u8]) -> Vec<Control> {
    let radius = 9.0;
    shadow(painter, unit, radius);
    metal(painter, unit, radius, fx.face, fx.body);
    brushed(painter, unit);
    painter.rect_stroke(unit, radius, stroke(1.0, Color32::from_black_alpha(120)));
    painter.line_segment(
        [
            unit.left_top() + Vec2::new(7.0, 1.0),
            unit.right_top() + Vec2::new(-7.0, 1.0),
        ],
        stroke(1.0, Color32::from_white_alpha(55)),
    );
    for corner in [
        unit.left_top() + Vec2::new(8.0, 8.0),
        unit.right_top() + Vec2::new(-8.0, 8.0),
        unit.left_bottom() + Vec2::new(8.0, -8.0),
        unit.right_bottom() + Vec2::new(-8.0, -8.0),
    ] {
        screw(painter, corner);
    }
    jacks(painter, unit, fx.edge);
    legends(painter, unit, fx);
    knob_row(painter, fx, unit, values);
    led(
        painter,
        art,
        unit.right_top() + Vec2::new(-15.0, 15.0),
        fx.led,
        fx.knobs > 0,
    );
    footswitch(painter, Pos2::new(unit.center().x, unit.bottom() - 23.0));
    fx.controls(unit)
}

fn treadle(painter: &Painter, art: &Art, unit: Rect, fx: &Pedal, _values: &[u8]) -> Vec<Control> {
    let radius = 10.0;
    shadow(painter, unit, radius);
    metal(painter, unit, radius, fx.face, fx.body);
    painter.rect_stroke(unit, radius, stroke(1.0, Color32::from_black_alpha(140)));
    let plate = Rect::from_min_max(
        unit.left_top() + Vec2::new(16.0, 28.0),
        unit.right_bottom() + Vec2::new(-12.0, -42.0),
    );
    painter.add(Shape::convex_polygon(
        vec![
            plate.left_top() + Vec2::new(4.0, 0.0),
            plate.right_top(),
            plate.right_bottom(),
            plate.left_bottom(),
        ],
        Color32::from_rgb(24, 24, 26),
        stroke(1.0, Color32::from_black_alpha(160)),
    ));
    for step in 0..7 {
        let t = (step as f32 + 0.7) / 8.0;
        let y = plate.top() + plate.height() * t;
        let left = plate.left() + 4.0 * (1.0 - t);
        painter.line_segment(
            [Pos2::new(left + 6.0, y), Pos2::new(plate.right() - 6.0, y)],
            stroke(1.0, Color32::from_white_alpha(16)),
        );
    }
    let hinge = Rect::from_min_max(
        unit.left_top() + Vec2::new(20.0, 20.0),
        unit.right_top() + Vec2::new(-20.0, 28.0),
    );
    metal(
        painter,
        hinge,
        3.0,
        Color32::from_rgb(160, 162, 166),
        Color32::from_rgb(72, 74, 78),
    );
    screw(painter, unit.left_top() + Vec2::new(9.0, 9.0));
    screw(painter, unit.right_top() + Vec2::new(-9.0, 9.0));
    front_face(painter, unit, fx);
    led(
        painter,
        art,
        unit.right_bottom() + Vec2::new(-20.0, -20.0),
        fx.led,
        true,
    );
    Vec::new()
}

fn front_face(painter: &Painter, unit: Rect, fx: &Pedal) {
    spaced(
        painter,
        Pos2::new(unit.center().x, unit.bottom() - 29.0),
        Align2::CENTER_CENTER,
        fx.name,
        semibold(10.0),
        fx.ink,
        1.4,
    );
    spaced(
        painter,
        Pos2::new(unit.center().x, unit.bottom() - 18.0),
        Align2::CENTER_CENTER,
        fx.brand,
        semibold(6.0),
        fx.ink.gamma_multiply(0.65),
        1.4,
    );
}

pub(super) fn paint(
    painter: &Painter,
    art: &Art,
    area: Rect,
    fx: &Pedal,
    values: &[u8],
) -> Vec<Control> {
    let (width, height): (f32, f32) = match fx.form {
        Form::Box => (148.0, 166.0),
        Form::Wah => (172.0, 166.0),
        Form::Tape => (238.0, 112.0),
        Form::Tank => (246.0, 104.0),
        Form::Rack => (248.0, 88.0),
    };
    let unit = Rect::from_center_size(
        area.center() + Vec2::new(0.0, 2.0),
        Vec2::new(
            width.min(area.width() - 6.0),
            height.min(area.height() - 4.0),
        ),
    );
    match fx.form {
        Form::Box => stomp(painter, art, unit, fx, values),
        Form::Wah => treadle(painter, art, unit, fx, values),
        Form::Tape => {
            super::units::tape(painter, art, unit, fx, values);
            fx.controls(unit)
        }
        Form::Tank => {
            super::units::tank(painter, art, unit, fx, values);
            fx.controls(unit)
        }
        Form::Rack => {
            super::units::rack(painter, art, unit, fx, values);
            fx.controls(unit)
        }
    }
}

/// A vacant slot on the board: a footprint that invites a click.
pub(super) fn empty(painter: &Painter, area: Rect, editable: bool) {
    let slot = Rect::from_center_size(area.center(), Vec2::new(104.0, 140.0));
    let dash = Color32::from_white_alpha(if editable { 30 } else { 20 });
    dash_line(painter, slot.left_top(), slot.right_top(), dash);
    dash_line(painter, slot.left_bottom(), slot.right_bottom(), dash);
    dash_line(painter, slot.left_top(), slot.left_bottom(), dash);
    dash_line(painter, slot.right_top(), slot.right_bottom(), dash);

    for t in [0.28, 0.5, 0.72] {
        painter.circle_stroke(
            Pos2::new(slot.left() + slot.width() * t, slot.top() + 48.0),
            8.0,
            stroke(1.0, Color32::from_white_alpha(13)),
        );
    }
    painter.circle_stroke(
        Pos2::new(slot.center().x, slot.bottom() - 26.0),
        10.0,
        stroke(1.0, Color32::from_white_alpha(13)),
    );

    // A plus in a circle is the obvious "click here to add something"; a
    // stored slot that cannot be written yet keeps the plain footprint.
    let ink = if editable { TEXT_2 } else { TEXT_3 };
    if editable {
        let plus = Pos2::new(slot.center().x, slot.top() + 48.0);
        painter.circle_stroke(plus, 13.0, stroke(1.0, ink.gamma_multiply(0.55)));
        painter.line_segment(
            [plus - Vec2::new(5.0, 0.0), plus + Vec2::new(5.0, 0.0)],
            stroke(1.2, ink),
        );
        painter.line_segment(
            [plus - Vec2::new(0.0, 5.0), plus + Vec2::new(0.0, 5.0)],
            stroke(1.2, ink),
        );
    }
    spaced(
        painter,
        Pos2::new(slot.center().x, slot.top() + 14.0),
        Align2::CENTER_CENTER,
        if editable { "LOAD EFFECT" } else { "EMPTY" },
        semibold(8.5),
        ink,
        1.6,
    );
}

fn dash_line(painter: &Painter, from: Pos2, to: Pos2, color: Color32) {
    let delta = to - from;
    let length = delta.length();
    let dir = delta / length.max(1.0);
    let mut along = 0.0;
    while along < length {
        let end = (along + 5.0).min(length);
        painter.line_segment([from + dir * along, from + dir * end], stroke(1.0, color));
        along += 11.0;
    }
}
