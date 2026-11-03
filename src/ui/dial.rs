//! Knobs on the panel: the dial, its scale, dragging, adding and removing.

use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Painter, Pos2, Rect, Sense, Ui, Vec2};

use super::app::Knob;
use super::art::{Art, KNOB_D};
use super::knobs::{add_knob, MAX_KNOBS};
use super::paint::{full_uv, snapped, spaced};
use super::theme::*;
use super::App;
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

impl App {
    pub(super) fn add_button(&mut self, ui: &mut Ui, painter: &Painter, panel: Rect) {
        let center = panel.right_top() + Vec2::new(-34.0, 26.0);
        let rect = Rect::from_center_size(center, Vec2::splat(30.0));
        let popup = Id::new("add-knob-menu");
        let open = ui.memory(|memory| memory.is_popup_open(popup));
        let mut response = ui
            .interact(rect, Id::new("add-knob"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if !open {
            response = response.on_hover_text("Add a knob to the panel");
        }

        // Stays out of the way until the pointer is on the panel.
        if open || ui.rect_contains_pointer(panel) {
            let ink = if response.hovered() || open {
                SILK
            } else {
                SILK_DIM.gamma_multiply(0.7)
            };
            painter.circle_stroke(center, 11.0, stroke(1.0, ink));
            painter.line_segment(
                [center - Vec2::new(4.5, 0.0), center + Vec2::new(4.5, 0.0)],
                stroke(1.25, ink),
            );
            painter.line_segment(
                [center - Vec2::new(0.0, 4.5), center + Vec2::new(0.0, 4.5)],
                stroke(1.25, ink),
            );
        }

        if response.clicked() {
            ui.memory_mut(|memory| memory.toggle_popup(popup));
        }
        let mut chosen = None;
        egui::popup_below_widget(ui, popup, &response, |ui| {
            ui.set_min_width(170.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            // Front-panel knobs first, then what only direct control reaches.
            for (standard, heading) in [(true, "On the Amp"), (false, "Hidden (Guessed)")] {
                let mut specs = params::KNOBS
                    .iter()
                    .filter(|spec| spec.standard == standard)
                    .filter(|spec| self.knobs.iter().all(|knob| knob.offset != spec.offset))
                    .peekable();
                if specs.peek().is_none() {
                    continue;
                }
                ui.label(
                    egui::RichText::new(heading)
                        .font(semibold(11.0))
                        .color(TEXT_3),
                );
                for spec in specs {
                    if ui
                        .selectable_label(false, spec.name)
                        .on_hover_text(spec.hint)
                        .clicked()
                    {
                        chosen = Some(spec);
                    }
                }
            }
        });
        if let Some(spec) = chosen {
            add_knob(&mut self.knobs, spec.offset);
        }
    }
}

pub(super) fn knob_widget(
    ui: &mut Ui,
    painter: &Painter,
    art: &Art,
    slot: Rect,
    knob: &Knob,
    value: &mut u8,
    baseline: u8,
) -> egui::Response {
    let center = slot.center_top() + Vec2::new(0.0, 20.0 + DIAL_R);
    let id = Id::new(("knob", knob.offset));
    let voice = (44..=46).contains(&knob.offset);
    let range = if voice { 13.0 } else { 255.0 };

    let response = ui.interact(
        Rect::from_center_size(center, Vec2::splat(DIAL_R * 1.7)),
        id,
        Sense::click_and_drag(),
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
        SILK,
        1.4,
    );

    dial_scale(painter, center, voice);

    let body = snapped(Rect::from_center_size(center, Vec2::splat(KNOB_D)), art.ppp);
    painter.image(
        art.glow.id(),
        body.expand(12.0).translate(Vec2::new(0.0, 5.0)),
        full_uv(),
        Color32::from_black_alpha(235),
    );
    painter.image(art.knob.id(), body, full_uv(), Color32::WHITE);

    // Pointer: white on the black skirt, dark on the silver top.
    let angle = SWEEP_START + (SWEEP_END - SWEEP_START) * t;
    let dir = Vec2::new(angle.cos(), angle.sin());
    let radius = KNOB_D / 2.0;
    painter.line_segment(
        [
            center + dir * (radius * 0.72),
            center + dir * (radius * 0.97),
        ],
        stroke(2.25, Color32::from_rgb(245, 243, 236)),
    );
    painter.line_segment(
        [
            center + dir * (radius * 0.2),
            center + dir * (radius * 0.57),
        ],
        stroke(1.75, Color32::from_rgb(38, 37, 36)),
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
        models::model_name(Kind::Amp, voice_model_id(*value))
            .unwrap_or("Unknown")
            .to_string()
    } else {
        format!("{:.1}", *value as f32 / 255.0 * 10.0)
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
        if edited { AMBER } else { SILK_DIM },
    );

    let hint = params::knob_spec(knob.offset)
        .map(|spec| format!("{}\n", spec.hint))
        .unwrap_or_default();
    response
        .on_hover_cursor(CursorIcon::ResizeVertical)
        .on_hover_text(format!(
            "{}: 0x{value:02x} ({value})\n{hint}Drag to adjust. Hold Shift for fine control.",
            knob.label,
            value = *value,
        ))
}

/// Tick marks and numerals silk-screened on the panel around a knob.
fn dial_scale(painter: &Painter, center: Pos2, voice: bool) {
    let positions = if voice { 14 } else { 11 };
    let radius = KNOB_D / 2.0;
    for tick in 0..positions {
        let t = tick as f32 / (positions - 1) as f32;
        let angle = SWEEP_START + (SWEEP_END - SWEEP_START) * t;
        let dir = Vec2::new(angle.cos(), angle.sin());
        painter.line_segment(
            [center + dir * (radius + 2.5), center + dir * (radius + 6.0)],
            stroke(1.0, SILK_DIM),
        );
        if !voice {
            painter.text(
                center + dir * (radius + 12.5),
                Align2::CENTER_CENTER,
                tick.to_string(),
                regular(9.5),
                SILK_DIM,
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
pub(super) fn remove_button(ui: &mut Ui, painter: &Painter, slot: Rect, offset: usize) -> bool {
    let center = slot.center_top() + Vec2::new(46.0, 9.0);
    let rect = Rect::from_center_size(center, Vec2::splat(18.0));
    let response = ui
        .interact(rect, Id::new(("knob-remove", offset)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Remove from panel");
    let ink = if response.hovered() { SILK } else { SILK_DIM };
    if response.hovered() {
        painter.circle_filled(center, 9.0, Color32::from_white_alpha(20));
    }
    let arm = Vec2::splat(3.5);
    painter.line_segment([center - arm, center + arm], stroke(1.25, ink));
    let other = Vec2::new(3.5, -3.5);
    painter.line_segment([center - other, center + other], stroke(1.25, ink));
    response.clicked()
}

fn voice_model_id(index: u8) -> u16 {
    match index {
        0x00 => 0x00eb,
        0x01 => 0x0067,
        0x02 => 0x0064,
        0x03 => 0x0053,
        0x04 => 0x006a,
        0x05 => 0x0075,
        0x06 => 0x0072,
        0x07 => 0x0061,
        0x08 => 0x006d,
        0x09 => 0x005e,
        0x0a => 0x005d,
        0x0b => 0x0079,
        0x0c => 0x007c,
        0x0d => 0x00ea,
        _ => 0,
    }
}
