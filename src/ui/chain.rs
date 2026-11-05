//! The effects around the amp, sitting on a pedalboard.

use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Ui, Vec2};

use super::art::Art;
use super::board;
use super::dial::{drag_value, scale10};
use super::paint::spaced;
use super::pedal;
use super::theme::*;
use super::{fx, App};
use crate::models;
use crate::protocol::{self, Kind, BANK_LIVE};

pub(super) const PEDAL_H: f32 = 224.0;

const PAD: f32 = 12.0;

const HEADER: f32 = 26.0;

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
        let mut edits = Vec::new();
        let mut toggles = Vec::new();

        for (index, (kind, title)) in chain.into_iter().enumerate() {
            let slot = Rect::from_min_size(
                Pos2::new(inner.left() + index as f32 * column, inner.top()),
                Vec2::new(column, inner.height()),
            );
            let response = ui
                .interact(slot, Id::new(("fx-slot", kind.label())), Sense::click())
                .on_hover_cursor(if editable {
                    CursorIcon::PointingHand
                } else {
                    CursorIcon::Default
                });
            let popup = Id::new(("fx-menu", kind.label()));
            let block = preset.block(kind);
            let model = block.and_then(|message| message.model()).unwrap_or(0);
            let loaded = model != 0;
            let bypassed = block.is_some_and(|message| message.byte(protocol::OFF_BYPASS) != 0);
            let name = if loaded {
                models::model_name(kind, model).unwrap_or("Unknown")
            } else {
                "Empty"
            };
            spaced(
                &painter,
                slot.left_top() + Vec2::new(4.0, 2.0),
                Align2::LEFT_TOP,
                &title.to_uppercase(),
                semibold(9.0),
                if response.hovered() && editable {
                    TEXT_2
                } else {
                    TEXT_3
                },
                1.7,
            );
            let dot = slot.right_top() + Vec2::new(-10.0, 10.0);
            if loaded && !bypassed {
                painter.circle_filled(dot, 6.0, AMBER.gamma_multiply(0.16));
                painter.circle_filled(dot, 2.8, AMBER);
            } else {
                painter.circle_filled(dot, 2.4, RAISED);
            }

            // A loaded effect can be switched off without unloading it, so its
            // model and settings stay exactly where they are.
            let mut power_clicked = false;
            if loaded && editable {
                let pill = Rect::from_center_size(
                    slot.right_top() + Vec2::new(-36.0, 10.0),
                    Vec2::new(42.0, 17.0),
                );
                let response = ui
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
                painter.rect_filled(pill, 8.0, if response.hovered() { hover } else { fill });
                painter.rect_stroke(pill, 8.0, stroke(1.0, ink.gamma_multiply(0.45)));
                painter.text(
                    pill.center(),
                    Align2::CENTER_CENTER,
                    if bypassed { "OFF" } else { "ON" },
                    semibold(8.5),
                    ink,
                );
                if response.clicked() {
                    power_clicked = true;
                    toggles.push((kind, u8::from(!bypassed)));
                }
            }

            // Keep a frame per slot so a drag only touches this block, and the
            // pixels on the pedal reflect the bytes the amp will receive.
            let mut frame = block.map(|message| *message.as_bytes()).unwrap_or([0; 64]);
            let before = frame;
            let look = fx::look(kind, model).filter(|_| loaded);
            let area = Rect::from_min_max(
                Pos2::new(slot.left() + 2.0, slot.top() + HEADER),
                Pos2::new(slot.right() - 2.0, slot.bottom() - 2.0),
            );
            let controls = match &look {
                Some(look) => {
                    pedal::paint(&painter, art, area, look, &frame[protocol::OFF_PARAMS..])
                }
                None => {
                    pedal::empty(&painter, area, editable);
                    Vec::new()
                }
            };
            if loaded && bypassed {
                painter.rect_filled(area.shrink(4.0), 10.0, Color32::from_black_alpha(90));
            }

            let params = look.as_ref().map(|look| look.params()).unwrap_or_default();
            let baseline = self.effect_baseline(preset, kind);
            let mut knob_hovered = false;
            for (knob, control) in controls.iter().enumerate() {
                let rect =
                    Rect::from_center_size(control.center, Vec2::splat(control.radius * 3.0));
                let response = ui.interact(
                    rect,
                    Id::new(("fx-knob", kind.label(), knob)),
                    if editable {
                        Sense::click_and_drag()
                    } else {
                        Sense::hover()
                    },
                );
                let (label, offset) = params
                    .get(knob)
                    .map(|param| (param.label, param.offset))
                    .unwrap_or(("PARAM", protocol::OFF_PARAMS + knob));
                if editable {
                    drag_value(ui, &response, &mut frame[offset], 255.0, 1.2);
                    if response.dragged() {
                        ui.ctx().request_repaint();
                    }
                }
                knob_hovered |= response.hovered() || response.dragged();

                let value = frame[offset];
                let edited = value != baseline[offset];
                if edited {
                    painter.circle_stroke(
                        control.center,
                        control.radius + 2.0,
                        stroke(1.0, AMBER.gamma_multiply(0.65)),
                    );
                }
                if response.hovered() || response.dragged() {
                    painter.circle_stroke(
                        control.center,
                        control.radius + 3.0,
                        stroke(1.4, Color32::from_white_alpha(120)),
                    );
                }
                if response.hovered() || response.dragged() {
                    painter.text(
                        control.center - Vec2::new(0.0, control.radius + 7.0),
                        Align2::CENTER_BOTTOM,
                        scale10(value),
                        semibold(10.5),
                        if edited { AMBER } else { TEXT },
                    );
                }
                response
                    .on_hover_cursor(if editable {
                        CursorIcon::ResizeVertical
                    } else {
                        CursorIcon::Default
                    })
                    .on_hover_text(if editable {
                        format!(
                            "{label}: {} / 10\nEffect parameter {offset} — the mapping is not verified.\nDrag to adjust. Hold Shift for fine control.",
                            scale10(value)
                        )
                    } else {
                        format!(
                            "{label}: {} / 10\nChoose the Live Buffer to edit.",
                            scale10(value)
                        )
                    });
            }
            if frame != before && editable {
                edits.push((kind, before, frame));
            }

            if response.hovered() && editable {
                if !loaded {
                    painter.rect_filled(slot.shrink(2.0), 8.0, Color32::from_white_alpha(5));
                }
                painter.line_segment(
                    [
                        Pos2::new(slot.left() + 4.0, slot.top() + HEADER - 1.0),
                        Pos2::new(slot.right() - 4.0, slot.top() + HEADER - 1.0),
                    ],
                    stroke(1.0, Color32::from_white_alpha(22)),
                );
            }

            // The cross only appears while the slot is hovered, so it does not
            // compete with the effect's own hardware.
            let mut remove = false;
            if loaded && editable && response.hovered() {
                let center = slot.right_top() + Vec2::new(-26.0, 10.0);
                let cross = ui
                    .interact(
                        Rect::from_center_size(center, Vec2::splat(18.0)),
                        Id::new(("fx-remove", kind.label())),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("Remove this effect");
                let ink = if cross.hovered() { AMBER } else { TEXT_3 };
                let arm = Vec2::splat(3.0);
                painter.line_segment([center - arm, center + arm], stroke(1.0, ink));
                painter.line_segment(
                    [center - Vec2::new(3.0, -3.0), center + Vec2::new(3.0, -3.0)],
                    stroke(1.0, ink),
                );
                if cross.clicked() {
                    remove = true;
                }
            }

            if response.clicked() && editable && !knob_hovered && !remove && !power_clicked {
                ui.memory_mut(|memory| memory.toggle_popup(popup));
            }

            let tip = if loaded {
                let slot_text = block
                    .and_then(|message| message.slot())
                    .map(|slot| format!(", slot {slot}"))
                    .unwrap_or_default();
                let action = if editable {
                    "Click to change the effect. Drag the knobs to adjust. Right-click to remove."
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

            let mut chosen = remove.then_some(0u16);
            let mut open_menu = false;
            let anchor = response.clone();
            if editable {
                let _ = response.context_menu(|ui| {
                    ui.set_min_width(186.0);
                    let change = if loaded {
                        "Change effect…"
                    } else {
                        "Load effect…"
                    };
                    if ui.button(change).clicked() {
                        open_menu = true;
                        ui.close_menu();
                    }
                    if loaded && ui.button("Remove effect").clicked() {
                        chosen = Some(0);
                        ui.close_menu();
                    }
                });
            }

            if open_menu {
                ui.memory_mut(|memory| memory.toggle_popup(popup));
            }
            egui::popup_above_or_below_widget(
                ui,
                popup,
                &anchor,
                egui::AboveOrBelow::Above,
                |ui| {
                    ui.set_min_width(190.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        egui::RichText::new(title)
                            .font(semibold(11.0))
                            .color(TEXT_3),
                    );
                    ui.label(
                        egui::RichText::new("SCX2 = seen in the bundled capture")
                            .font(regular(9.5))
                            .color(TEXT_3),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(ui, |ui| {
                            ui.set_min_width(190.0);
                            for (id, label) in models::table(kind).unwrap_or(&[]) {
                                let text = if models::observed(kind, *id) {
                                    format!("{label}   SCX2")
                                } else {
                                    label.to_string()
                                };
                                if ui.selectable_label(*id == model, text).clicked() {
                                    chosen = Some(*id);
                                    ui.close_menu();
                                }
                            }
                        });
                },
            );
            if let Some(id) = chosen {
                loads.push((kind, id));
            }
        }

        for (kind, model) in loads {
            self.set_effect_model(kind, model);
        }
        for (kind, value) in toggles {
            self.apply_edit(kind, protocol::OFF_BYPASS, value);
        }
        for (kind, before, after) in edits {
            self.apply_effect(
                kind,
                &before[protocol::OFF_PARAMS..],
                &after[protocol::OFF_PARAMS..],
            );
        }
    }
}
