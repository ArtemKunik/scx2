//! Shared hardware for the effect renders: metal, knobs, switches, jacks.

use eframe::egui::{
    epaint::Shadow, Align2, Color32, Mesh, Painter, Pos2, Rect, Rounding, Shape, Vec2,
};

use super::art::Art;
use super::fx::Pedal;
use super::paint::{full_uv, spaced};
use super::theme::stroke;

pub(super) fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// A rounded rectangle filled with a smooth vertical gradient.
pub(super) fn metal(painter: &Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    let steps = 20usize;
    let mut mesh = Mesh::default();
    for row in 0..=steps {
        let t = row as f32 / steps as f32;
        let y = rect.top() + rect.height() * t;
        let inset = corner_inset(rect, radius, y);
        let color = lerp(top, bottom, t);
        mesh.colored_vertex(Pos2::new(rect.left() + inset, y), color);
        mesh.colored_vertex(Pos2::new(rect.right() - inset, y), color);
    }
    for row in 0..steps {
        let a = (row * 2) as u32;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 2, a + 3);
    }
    painter.add(Shape::mesh(mesh));
}

fn corner_inset(rect: Rect, radius: f32, y: f32) -> f32 {
    let r = radius.min(rect.height() / 2.0);
    let d = if y < rect.top() + r {
        r - (y - rect.top())
    } else if y > rect.bottom() - r {
        y - (rect.bottom() - r)
    } else {
        return 0.0;
    };
    r - (r * r - d * d).max(0.0).sqrt()
}

pub(super) fn shadow(painter: &Painter, rect: Rect, radius: f32) {
    painter.add(Shape::mesh(
        Shadow {
            offset: Vec2::new(0.0, 5.0),
            blur: 14.0,
            spread: 0.0,
            color: Color32::from_black_alpha(150),
        }
        .tessellate(rect, Rounding::same(radius)),
    ));
}

pub(super) fn brushed(painter: &Painter, rect: Rect) {
    let left = rect.left() + 5.0;
    let right = rect.right() - 5.0;
    for row in 0..10 {
        let y = rect.top() + 5.0 + row as f32 * (rect.height() - 10.0) / 10.0;
        let color = if row % 2 == 0 {
            Color32::from_white_alpha(7)
        } else {
            Color32::from_black_alpha(14)
        };
        painter.line_segment(
            [Pos2::new(left, y), Pos2::new(right, y)],
            stroke(0.6, color),
        );
    }
}

pub(super) fn small_knob(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    body: Color32,
    cap: Color32,
    angle: f32,
) {
    painter.circle_filled(
        center + Vec2::new(0.0, 1.4),
        radius + 1.2,
        Color32::from_black_alpha(120),
    );
    painter.circle_filled(center, radius, body);
    painter.circle_filled(center - Vec2::new(0.5, 0.8), radius * 0.64, cap);
    painter.circle_stroke(center, radius, stroke(0.8, Color32::from_white_alpha(38)));
    let tip = if (cap.r() as u16 + cap.g() as u16 + cap.b() as u16) > 380 {
        Color32::from_rgb(26, 24, 22)
    } else {
        Color32::from_rgb(240, 238, 232)
    };
    let dir = Vec2::new(angle.cos(), angle.sin());
    painter.line_segment(
        [
            center + dir * (radius * 0.16),
            center + dir * (radius * 0.92),
        ],
        stroke(1.2, tip),
    );
}

pub(super) fn led(painter: &Painter, art: &Art, center: Pos2, color: Color32, lit: bool) {
    if lit {
        painter.image(
            art.glow.id(),
            Rect::from_center_size(center, Vec2::splat(22.0)),
            full_uv(),
            color.gamma_multiply(0.45),
        );
        painter.circle_filled(center, 3.4, color);
        painter.circle_filled(center, 1.6, Color32::WHITE);
    } else {
        painter.circle_filled(center, 3.0, color.gamma_multiply(0.18));
    }
}

pub(super) fn footswitch(painter: &Painter, center: Pos2) {
    painter.circle_filled(
        center + Vec2::new(0.0, 1.5),
        12.5,
        Color32::from_black_alpha(130),
    );
    painter.circle_filled(center, 11.5, Color32::from_rgb(96, 98, 102));
    painter.circle_filled(center, 8.0, Color32::from_rgb(54, 55, 58));
    painter.circle_stroke(center, 11.5, stroke(0.8, Color32::from_white_alpha(60)));
    painter.circle_stroke(center, 5.6, stroke(0.7, Color32::from_black_alpha(130)));
    for turn in 0..8 {
        let angle = turn as f32 * std::f32::consts::TAU / 8.0;
        let dir = Vec2::new(angle.cos(), angle.sin());
        painter.circle_filled(center + dir * 9.7, 0.8, Color32::from_black_alpha(90));
    }
}

pub(super) fn jacks(painter: &Painter, unit: Rect, edge: Color32) {
    let y = unit.center().y + 2.0;
    for x in [unit.left() + 0.5, unit.right() - 0.5] {
        let center = Pos2::new(x, y);
        painter.circle_filled(center, 6.5, edge);
        painter.circle_filled(center, 4.2, Color32::from_rgb(18, 18, 20));
        painter.circle_stroke(center, 5.4, stroke(0.7, Color32::from_white_alpha(55)));
    }
}

pub(super) fn legends(painter: &Painter, unit: Rect, fx: &Pedal) {
    spaced(
        painter,
        unit.center_top() + Vec2::new(0.0, 7.0),
        Align2::CENTER_TOP,
        fx.brand,
        super::theme::semibold(6.0),
        fx.ink.gamma_multiply(0.75),
        1.6,
    );
    spaced(
        painter,
        Pos2::new(unit.center().x, unit.top() + unit.height() * 0.56),
        Align2::CENTER_CENTER,
        fx.name,
        super::theme::semibold(10.0),
        fx.ink,
        1.0,
    );
    spaced(
        painter,
        Pos2::new(unit.center().x, unit.top() + unit.height() * 0.66),
        Align2::CENTER_CENTER,
        fx.sub,
        super::theme::semibold(5.8),
        fx.accent.gamma_multiply(0.9),
        1.2,
    );
}

/// Where a knob's travel begins and ends, in radians from the +x axis.
///
/// Real pedal knobs stop at about 7 o'clock and 4 o'clock: a 270-degree sweep
/// through 12 o'clock, not a free spin. Screen coordinates grow downwards, so
/// clockwise is an increasing angle.
pub(super) const KNOB_START: f32 = 3.0 * std::f32::consts::FRAC_PI_4;
pub(super) const KNOB_SWEEP: f32 = 3.0 * std::f32::consts::FRAC_PI_2;

/// Map a parameter byte to the pointer angle on a small knob.
pub(super) fn knob_angle(value: u8) -> f32 {
    KNOB_START + value as f32 / 255.0 * KNOB_SWEEP
}

pub(super) fn knob_row(painter: &Painter, fx: &Pedal, unit: Rect, values: &[u8]) {
    for (index, control) in fx.controls(unit).iter().enumerate() {
        let value = values.get(index).copied().unwrap_or(0x80);
        small_knob(
            painter,
            control.center,
            control.radius,
            fx.knob,
            fx.knob_cap,
            knob_angle(value),
        );
        if let Some(label) = fx.labels.get(index) {
            spaced(
                painter,
                control.center + Vec2::new(0.0, control.radius + 4.0),
                Align2::CENTER_TOP,
                label,
                super::theme::semibold(6.2),
                fx.ink.gamma_multiply(0.8),
                0.9,
            );
        }
    }
}

pub(super) fn reels(painter: &Painter, window: Rect, body: Color32) {
    let centers = [
        Pos2::new(window.left() + window.width() * 0.28, window.center().y),
        Pos2::new(window.left() + window.width() * 0.72, window.center().y),
    ];
    painter.line_segment(
        [
            Pos2::new(centers[0].x, window.center().y + 10.0),
            Pos2::new(centers[1].x, window.center().y + 10.0),
        ],
        stroke(1.5, Color32::from_rgb(120, 100, 70)),
    );
    for center in centers {
        let radius = (window.height() * 0.38).min(17.0);
        painter.circle_filled(center, radius, Color32::from_rgb(20, 20, 22));
        painter.circle_stroke(
            center,
            radius,
            stroke(1.4, Color32::from_rgb(150, 146, 132)),
        );
        painter.circle_filled(center, radius * 0.22, body);
        for spoke in 0..6 {
            let angle = spoke as f32 * std::f32::consts::TAU / 6.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            painter.line_segment(
                [
                    center + dir * (radius * 0.26),
                    center + dir * (radius * 0.88),
                ],
                stroke(1.2, Color32::from_rgb(128, 124, 112)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{knob_angle, KNOB_START, KNOB_SWEEP};

    #[test]
    fn a_knob_turns_through_exactly_270_degrees() {
        let start = knob_angle(0);
        let end = knob_angle(255);
        assert!(
            (start - KNOB_START).abs() < 1e-6,
            "a knob at zero points to 7 o'clock"
        );
        assert!(
            ((end - start) - KNOB_SWEEP).abs() < 1e-5,
            "full travel is 270 degrees"
        );
    }
}
