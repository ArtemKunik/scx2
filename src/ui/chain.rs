//! The effects around the amp, sitting on a pedalboard.
//!
//! The board is an overview, not a control surface: each slot is one large
//! click target that opens the focused effect editor. The knobs drawn on the
//! pedal are decoration; the working controls live in the editor, where they
//! can be made big enough to read and hit.

use eframe::egui::{Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Ui, Vec2};

use super::art::Art;
use super::board;
use super::paint::spaced;
use super::pedal;
use super::theme::*;
use super::{fx, App};
use crate::models;
use crate::protocol::{self, Kind, BANK_LIVE};

pub(super) const PEDAL_H: f32 = 248.0;

const PAD: f32 = 12.0;

const HEADER: f32 = 30.0;

impl App {
    pub(super) fn pedals(&mut self, ui: &mut Ui, art: &Art, preset: &protocol::Preset, width: f32) {
        let (board, _) = ui.allocate_exact_size(Vec2::new(width, PEDAL_H), Sense::hover());
        let painter = ui.painter().clone();
        board::paint(&painter, art, board);

        // A stored slot cannot be written yet, so only the live buffer (or an
        // offline file) can be edited.
        let editable = !self.connected || preset.bank == BANK_LIVE;
        let inner = board.shrink(PAD);
        let column = inner.width() / 4.0;
        let chain = [
            (Kind::Stomp, "Stomp"),
            (Kind::Mod, "Modulation"),
            (Kind::Delay, "Delay"),
            (Kind::Reverb, "Reverb"),
        ];
        let mut loads = Vec::new();
        let mut toggles = Vec::new();
        let mut opens = Vec::new();

        for (index, (kind, title)) in chain.into_iter().enumerate() {
            let slot = Rect::from_min_size(
                Pos2::new(inner.left() + index as f32 * column, inner.top()),
                Vec2::new(column, inner.height()),
            );
            let block = preset.block(kind);
            let model = block.and_then(|message| message.model()).unwrap_or(0);
            let loaded = model != 0;
            let can_open = editable || loaded;
            let response = ui
                .interact(slot, Id::new(("fx-slot", kind.label())), Sense::click())
                .on_hover_cursor(if can_open {
                    CursorIcon::PointingHand
                } else {
                    CursorIcon::Default
                });
            let bypassed = block.is_some_and(|message| message.byte(protocol::OFF_BYPASS) != 0);
            let name = if loaded {
                models::model_name(kind, model).unwrap_or("Unknown")
            } else {
                "Empty"
            };

            // The board says what is in the slot and how to get at it. The
            // whole slot is the button, not the little hardware drawn on it.
            spaced(
                &painter,
                slot.left_top() + Vec2::new(6.0, 4.0),
                Align2::LEFT_TOP,
                &title.to_uppercase(),
                semibold(9.5),
                if response.hovered() { TEXT_2 } else { TEXT_3 },
                1.7,
            );
            spaced(
                &painter,
                slot.left_top() + Vec2::new(6.0, 17.0),
                Align2::LEFT_TOP,
                if loaded {
                    if editable {
                        "CLICK TO ADJUST"
                    } else {
                        "CLICK TO VIEW"
                    }
                } else if editable {
                    "CLICK TO LOAD"
                } else {
                    "EMPTY"
                },
                semibold(7.0),
                if response.hovered() && editable {
                    AMBER.gamma_multiply(0.9)
                } else {
                    TEXT_3.gamma_multiply(0.8)
                },
                1.3,
            );

            // A loaded effect can be switched off without unloading it, so its
            // model and settings stay exactly where they are.
            let mut power_clicked = false;
            if loaded && editable {
                let pill = Rect::from_center_size(
                    slot.right_top() + Vec2::new(-48.0, 13.0),
                    Vec2::new(42.0, 18.0),
                );
                let power = ui
                    .interact(pill, Id::new(("fx-power", kind.label())), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text(if bypassed {
                        "Switch this effect on. Its settings are kept."
                    } else {
                        "Switch this effect off. Its settings are kept."
                    });
                let (fill, hover, ink) = if bypassed {
                    (
                        Color32::from_white_alpha(10),
                        Color32::from_white_alpha(22),
                        TEXT_3,
                    )
                } else {
                    (
                        AMBER.gamma_multiply(0.18),
                        AMBER.gamma_multiply(0.28),
                        AMBER,
                    )
                };
                painter.rect_filled(pill, 9.0, if power.hovered() { hover } else { fill });
                painter.rect_stroke(pill, 9.0, stroke(1.0, ink.gamma_multiply(0.45)));
                painter.text(
                    pill.center(),
                    Align2::CENTER_CENTER,
                    if bypassed { "OFF" } else { "ON" },
                    semibold(8.5),
                    ink,
                );
                if power.clicked() {
                    power_clicked = true;
                    toggles.push((kind, u8::from(!bypassed)));
                }
            }

            // The cross sits away from the power pill, so a click on one never
            // lands on the other. It only appears while the slot is hovered.
            let mut remove = false;
            if loaded && editable && response.hovered() {
                let center = slot.right_top() + Vec2::new(-14.0, 13.0);
                let cross = ui
                    .interact(
                        Rect::from_center_size(center, Vec2::splat(18.0)),
                        Id::new(("fx-remove", kind.label())),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("Remove this effect");
                let ink = if cross.hovered() { AMBER } else { TEXT_3 };
                let arm = Vec2::splat(3.2);
                painter.line_segment([center - arm, center + arm], stroke(1.1, ink));
                painter.line_segment(
                    [center - Vec2::new(3.2, -3.2), center + Vec2::new(3.2, -3.2)],
                    stroke(1.1, ink),
                );
                if cross.clicked() {
                    remove = true;
                }
            }

            let frame = block.map(|message| *message.as_bytes()).unwrap_or([0; 64]);
            let look = fx::look(kind, model).filter(|_| loaded);
            let area = Rect::from_min_max(
                Pos2::new(slot.left() + 2.0, slot.top() + HEADER),
                Pos2::new(slot.right() - 2.0, slot.bottom() - 2.0),
            );
            if response.hovered() && can_open {
                painter.rect_filled(slot.shrink(3.0), 9.0, Color32::from_white_alpha(4));
                painter.rect_stroke(
                    slot.shrink(3.0),
                    9.0,
                    stroke(1.0, Color32::from_white_alpha(20)),
                );
                painter.line_segment(
                    [
                        Pos2::new(slot.left() + 6.0, slot.top() + HEADER - 2.0),
                        Pos2::new(slot.right() - 6.0, slot.top() + HEADER - 2.0),
                    ],
                    stroke(1.0, Color32::from_white_alpha(24)),
                );
            }
            match &look {
                Some(look) => {
                    pedal::paint(&painter, art, area, look, &frame[protocol::OFF_PARAMS..]);
                }
                None => {
                    pedal::empty(&painter, area, editable);
                }
            }
            if loaded && bypassed {
                painter.rect_filled(area.shrink(4.0), 10.0, Color32::from_black_alpha(90));
            }

            // A loaded pedal is opened from anywhere on the slot; a vacant one
            // opens straight into the model list.
            let mut open = false;
            let mut open_list = !loaded;
            let tip = if loaded {
                let slot_text = block
                    .and_then(|message| message.slot())
                    .map(|slot| format!(", slot {slot}"))
                    .unwrap_or_default();
                let action = if editable {
                    "Click for the big controls. Right-click for more."
                } else {
                    "Choose the Live Buffer to edit effects."
                };
                let state = if bypassed { " · off" } else { "" };
                format!("{name} — {title}{slot_text}{state}\n{action}")
            } else if editable {
                format!("{title}: click to load an effect")
            } else {
                format!("{title}: choose the Live Buffer to edit effects")
            };
            let response = response.on_hover_text(tip);

            if editable {
                let _ = response.context_menu(|ui| {
                    ui.set_min_width(190.0);
                    if ui
                        .button(if loaded {
                            "Open controls"
                        } else {
                            "Load effect…"
                        })
                        .clicked()
                    {
                        open = true;
                        ui.close_menu();
                    }
                    if loaded && ui.button("Change effect…").clicked() {
                        open = true;
                        open_list = true;
                        ui.close_menu();
                    }
                    if loaded && ui.button("Remove effect").clicked() {
                        remove = true;
                        ui.close_menu();
                    }
                });
            }
            if response.clicked() && can_open && !power_clicked && !remove {
                open = true;
            }
            if open {
                opens.push((kind, open_list));
            }
            if remove {
                loads.push((kind, 0));
            }
        }

        for (kind, model) in loads {
            self.set_effect_model(kind, model);
        }
        for (kind, value) in toggles {
            self.apply_edit(kind, protocol::OFF_BYPASS, value);
        }
        if let Some((kind, list)) = opens.into_iter().next() {
            self.fx_editor = Some(kind);
            self.fx_editor_list = list;
            self.fx_editor_armed = false;
        }
    }
}
