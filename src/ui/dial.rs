//! Knobs on the panel: the dial, its scale, dragging, adding and removing.

use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Painter, Pos2, Rect, Sense, Ui, Vec2};

use super::ampstyle::Look;
use super::app::Knob;
use super::art::{Art, KNOB_D};
use super::knobs::MAX_KNOBS;
use super::paint::{full_uv, snapped, spaced};
use super::theme::*;
use crate::protocol::Kind;
use crate::{models, params};

pub(super) const DIAL_R: f32 = 50.0;

/// Knob sweep, in radians clockwise from the +x axis.
const SWEEP_START: f32 = 135.0 * std::f32::consts::PI / 180.0;

const SWEEP_END: f32 = 405.0 * std::f32::consts::PI / 180.0;

/// True while the catalogue still has a knob that is not on the panel.
pub(super) fn can_add(knobs: &[Knob]) -> bool {
    knobs.len() < MAX_KNOBS
        && params::KNOBS
            .iter()
            .any(|spec| knobs.iter().all(|knob| knob.offset != spec.offset))
}

/// A knob's value, what the amp last reported, and whether it may be turned.
pub(super) struct Setting<'a> {
    pub(super) value: &'a mut u8,
    pub(super) baseline: u8,
    pub(super) editable: bool,
}

pub(super) fn knob_widget(
    ui: &mut Ui,
    painter: &Painter,
    art: &Art,
    look: &Look,
    slot: Rect,
    knob: &Knob,
    setting: Setting<'_>,
) -> egui::Response {
    let Setting {
        value,
        baseline,
        editable,
    } = setting;
    let center = slot.center_top() + Vec2::new(0.0, 20.0 + DIAL_R);
    let id = Id::new(("knob", knob.offset));
    let voice = (44..=46).contains(&knob.offset);
    let range = if voice { 13.0 } else { 255.0 };

    // Include the readout under the dial, so the model name is part of the
    // same control (and can be clicked to pick a voice).
    let hit = Rect::from_min_max(
        center - Vec2::new(DIAL_R * 0.85, DIAL_R * 0.85),
        center + Vec2::new(DIAL_R * 0.85, DIAL_R + 18.0),
    );
    let response = ui.interact(
        hit,
        id,
        if editable {
            Sense::click_and_drag()
        } else {
            Sense::click()
        },
    );
    drag_value(ui, &response, value, range, range / 170.0);

    // Preset changes sweep the pointer; a hand on the knob moves it directly.
    let target = (*value as f32 / range).clamp(0.0, 1.0);
    let eased = ui
        .ctx()
        .animate_value_with_time(id.with("sweep"), target, 0.18);
    let t = if response.dragged() { target } else { eased };
    let edited = *value != baseline;

    spaced(
        painter,
        slot.center_top() + Vec2::new(0.0, 2.0),
        Align2::CENTER_TOP,
        &knob.label.to_uppercase(),
        semibold(if knob.label.len() > 10 { 9.0 } else { 10.5 }),
        look.ink,
        1.4,
    );

    dial_scale(
        painter,
        center,
        if voice { 14 } else { 11 },
        !voice,
        look.ink,
    );

    let body = snapped(Rect::from_center_size(center, Vec2::splat(KNOB_D)), art.ppp);
    painter.image(
        art.glow.id(),
        body.expand(12.0).translate(Vec2::new(0.0, 5.0)),
        full_uv(),
        Color32::from_black_alpha(235),
    );
    painter.image(art.knob(look.knob).id(), body, full_uv(), Color32::WHITE);

    // Pointer: dark on a cream or silver cap, white on a black one.
    let angle = SWEEP_START + (SWEEP_END - SWEEP_START) * t;
    let dir = Vec2::new(angle.cos(), angle.sin());
    let radius = KNOB_D / 2.0;
    painter.line_segment(
        [
            center + dir * (radius * 0.72),
            center + dir * (radius * 0.97),
        ],
        stroke(2.25, look.pointer),
    );
    painter.line_segment(
        [
            center + dir * (radius * 0.2),
            center + dir * (radius * 0.57),
        ],
        stroke(1.75, look.pointer_shadow),
    );

    if response.hovered() || response.dragged() {
        // A breath of light on the metal says "this one is yours".
        painter.image(
            art.glow.id(),
            body.expand(10.0),
            full_uv(),
            Color32::from_rgba_premultiplied(26, 24, 20, 0),
        );
    }

    let readout = if voice {
        models::model_name(Kind::Amp, models::voice_model_id(*value))
            .unwrap_or("Unknown")
            .to_string()
    } else {
        scale10(*value)
    };
    painter.text(
        center + Vec2::new(0.0, DIAL_R - 8.0),
        Align2::CENTER_TOP,
        readout,
        if edited {
            semibold(12.5)
        } else {
            regular(12.5)
        },
        if edited {
            AMBER
        } else {
            look.ink.gamma_multiply(0.72)
        },
    );

    let hint = params::knob_spec(knob.offset)
        .map(|spec| format!("{}\n", spec.hint))
        .unwrap_or_default();
    let (cursor, how) = if editable && voice {
        (
            CursorIcon::PointingHand,
            "Click to choose an amp voice and speaker. Drag to sweep. Hold Shift for fine control.",
        )
    } else if editable {
        (
            CursorIcon::ResizeVertical,
            "Drag to adjust. Hold Shift for fine control.",
        )
    } else {
        (
            CursorIcon::Default,
            "Stored on the amp. Choose Live Buffer to turn this knob.",
        )
    };
    let value_text = format!("{} / 10", scale10(*value));
    response
        .on_hover_cursor(cursor)
        .on_hover_text(format!("{}: {value_text}\n{hint}{how}", knob.label,))
}

/// A raw 0..255 parameter as the 0..10 scale shown on every knob and cell.
///
/// The amp still receives the full byte; this is display only.
pub(super) fn scale10(value: u8) -> String {
    format!("{:.1}", value as f32 / 255.0 * 10.0)
}

/// Tick marks and (for continuous controls) numerals silk-screened around a knob.
fn dial_scale(painter: &Painter, center: Pos2, positions: usize, numbers: bool, ink: Color32) {
    let radius = KNOB_D / 2.0;
    let ink = ink.gamma_multiply(0.78);
    for tick in 0..positions {
        let t = tick as f32 / (positions - 1) as f32;
        let angle = SWEEP_START + (SWEEP_END - SWEEP_START) * t;
        let dir = Vec2::new(angle.cos(), angle.sin());
        painter.line_segment(
            [center + dir * (radius + 2.5), center + dir * (radius + 6.0)],
            stroke(1.0, ink),
        );
        if numbers {
            painter.text(
                center + dir * (radius + 12.5),
                Align2::CENTER_CENTER,
                tick.to_string(),
                regular(9.5),
                ink,
            );
        }
    }
}

/// Apply a drag. The value is tracked as a float while dragging: small moves
/// would otherwise round away, which froze knobs with few positions.
pub(super) fn drag_value(
    ui: &Ui,
    response: &egui::Response,
    value: &mut u8,
    range: f32,
    per_pixel: f32,
) {
    let key = response.id.with("drag-position");
    if response.drag_started() {
        ui.data_mut(|data| data.insert_temp(key, *value as f32));
    }
    if response.dragged() {
        let delta = response.drag_delta();
        let fine = if ui.input(|i| i.modifiers.shift) {
            0.2
        } else {
            1.0
        };
        let position = ui
            .data(|data| data.get_temp::<f32>(key))
            .unwrap_or(*value as f32);
        let next = (position + (delta.x - delta.y) * per_pixel * fine).clamp(0.0, range);
        ui.data_mut(|data| data.insert_temp(key, next));
        *value = next.round() as u8;
    }
}

/// Returns true when clicked.
pub(super) fn remove_button(
    ui: &mut Ui,
    painter: &Painter,
    slot: Rect,
    offset: usize,
    base: Color32,
) -> bool {
    let center = slot.center_top() + Vec2::new(46.0, 9.0);
    let rect = Rect::from_center_size(center, Vec2::splat(18.0));
    let response = ui
        .interact(rect, Id::new(("knob-remove", offset)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Remove from panel");
    let ink = if response.hovered() {
        base
    } else {
        base.gamma_multiply(0.7)
    };
    if response.hovered() {
        painter.circle_filled(center, 9.0, Color32::from_white_alpha(20));
    }
    let arm = Vec2::splat(3.5);
    painter.line_segment([center - arm, center + arm], stroke(1.25, ink));
    let other = Vec2::new(3.5, -3.5);
    painter.line_segment([center - other, center + other], stroke(1.25, ink));
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::scale10;

    #[test]
    fn raw_bytes_map_to_the_zero_to_ten_scale() {
        assert_eq!(scale10(0), "0.0");
        assert_eq!(scale10(128), "5.0");
        assert_eq!(scale10(200), "7.8");
        assert_eq!(scale10(255), "10.0");
    }
}
