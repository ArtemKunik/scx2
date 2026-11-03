//! The effects around the amp, shown as a row of pedals.

use eframe::egui::{Align2, Sense, Ui, Vec2};

use super::paint::elided;
use super::theme::*;
use crate::models;
use crate::protocol::{self, Kind};

pub(super) const PEDAL_H: f32 = 88.0;

pub(super) fn pedals(ui: &mut Ui, preset: &protocol::Preset, width: f32) {
    const GAP: f32 = 18.0;
    let pedal_w = (width - 3.0 * GAP) / 4.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let chain = [
            (Kind::Stomp, "Stomp"),
            (Kind::Mod, "Modulation"),
            (Kind::Delay, "Delay"),
            (Kind::Reverb, "Reverb"),
        ];
        for (index, (kind, title)) in chain.into_iter().enumerate() {
            pedal(ui, pedal_w, title, preset, kind);
            if index < 3 {
                let (gap, _) = ui.allocate_exact_size(Vec2::new(GAP, PEDAL_H), Sense::hover());
                ui.painter().line_segment(
                    [gap.left_center(), gap.right_center()],
                    stroke(1.0, HAIRLINE),
                );
            }
        }
    });
}

fn pedal(ui: &mut Ui, width: f32, title: &str, preset: &protocol::Preset, kind: Kind) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, PEDAL_H), Sense::hover());
    let painter = ui.painter();
    let block = preset.block(kind);
    let model = block.and_then(|message| message.model()).unwrap_or(0);
    let loaded = model != 0;

    painter.rect_filled(rect, 10.0, SIDEBAR);
    painter.rect_stroke(rect, 10.0, stroke(1.0, HAIRLINE));

    let led = rect.right_top() + Vec2::new(-16.0, 18.0);
    if loaded {
        painter.circle_filled(led, 7.0, AMBER.gamma_multiply(0.14));
        painter.circle_filled(led, 3.0, AMBER);
    } else {
        painter.circle_filled(led, 3.0, RAISED);
    }

    painter.text(
        rect.left_top() + Vec2::new(14.0, 12.0),
        Align2::LEFT_TOP,
        title,
        semibold(11.5),
        TEXT_3,
    );
    let name = if loaded {
        models::model_name(kind, model).unwrap_or("Unknown")
    } else {
        "Empty"
    };
    let ink = if loaded { TEXT } else { TEXT_3 };
    let galley = elided(painter, name, semibold(15.0), ink, width - 28.0);
    painter.galley(rect.left_top() + Vec2::new(14.0, 40.0), galley, ink);

    if let Some(slot) = block.and_then(|message| message.slot()) {
        response.on_hover_text(format!("{name}, slot {slot}"));
    }
}
