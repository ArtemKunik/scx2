//! The effects around the amp, sitting on a pedalboard.

use eframe::egui::{Align2, Color32, Id, Pos2, Rect, Sense, Ui, Vec2};

use super::ampstyle::Era;
use super::art::Art;
use super::paint::{screw, spaced, tile};
use super::pedal;
use super::theme::*;
use super::{fx, App};
use crate::models;
use crate::protocol::{self, Kind};

pub(super) const PEDAL_H: f32 = 224.0;

const PAD: f32 = 12.0;

const HEADER: f32 = 26.0;

impl App {
    pub(super) fn pedals(&mut self, ui: &mut Ui, art: &Art, preset: &protocol::Preset, width: f32) {
        let (board, _) = ui.allocate_exact_size(Vec2::new(width, PEDAL_H), Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(board, 12.0, Color32::from_rgb(23, 22, 20));
        tile(
            painter,
            board,
            12.0,
            &art.surface(Era::Metal).grille,
            art.ppp,
            Color32::from_gray(96),
        );
        painter.rect_stroke(board, 12.0, stroke(1.0, HAIRLINE));
        for corner in [
            board.left_top() + Vec2::new(10.0, 10.0),
            board.right_top() + Vec2::new(-10.0, 10.0),
            board.left_bottom() + Vec2::new(10.0, -10.0),
            board.right_bottom() + Vec2::new(-10.0, -10.0),
        ] {
            screw(painter, corner);
        }

        let inner = board.shrink(PAD);
        let column = inner.width() / 4.0;
        let chain = [
            (Kind::Stomp, "Stomp"),
            (Kind::Mod, "Modulation"),
            (Kind::Delay, "Delay"),
            (Kind::Reverb, "Reverb"),
        ];
        for (index, (kind, title)) in chain.into_iter().enumerate() {
            let slot = Rect::from_min_size(
                Pos2::new(inner.left() + index as f32 * column, inner.top()),
                Vec2::new(column, inner.height()),
            );
            let block = preset.block(kind);
            let model = block.and_then(|message| message.model()).unwrap_or(0);
            let loaded = model != 0;
            let name = if loaded {
                models::model_name(kind, model).unwrap_or("Unknown")
            } else {
                "Empty"
            };
            spaced(
                painter,
                slot.left_top() + Vec2::new(4.0, 2.0),
                Align2::LEFT_TOP,
                &title.to_uppercase(),
                semibold(9.0),
                TEXT_3,
                1.7,
            );
            let dot = slot.right_top() + Vec2::new(-10.0, 10.0);
            if loaded {
                painter.circle_filled(dot, 6.0, AMBER.gamma_multiply(0.16));
                painter.circle_filled(dot, 2.8, AMBER);
            } else {
                painter.circle_filled(dot, 2.4, RAISED);
            }

            let area = Rect::from_min_max(
                Pos2::new(slot.left() + 2.0, slot.top() + HEADER),
                Pos2::new(slot.right() - 2.0, slot.bottom() - 2.0),
            );
            match fx::look(kind, model).filter(|_| loaded) {
                Some(look) => pedal::paint(painter, art, area, &look),
                None => pedal::empty(painter, area),
            }

            let response = ui.interact(
                slot,
                Id::new(("fx-slot", kind.label())),
                Sense::hover(),
            );
            let tip = if loaded {
                if let Some(slot) = block.and_then(|message| message.slot()) {
                    format!("{name} — {title}, slot {slot}")
                } else {
                    format!("{name} — {title}")
                }
            } else {
                format!("{title}: no effect loaded in this preset")
            };
            response.on_hover_text(tip);
        }
    }
}
