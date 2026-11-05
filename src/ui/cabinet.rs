//! The amp itself: cabinet, control panel, pilot jewel and grille.

use eframe::egui::{
    self, epaint::Shadow, Align2, Color32, CursorIcon, Id, Painter, Pos2, Rect, Rounding, Sense,
    Shape, Ui, Vec2,
};

use super::ampstyle::{self, Look};
use super::app::Knob;
use super::art::{Art, JEWEL_D};
use super::dial::{can_add, knob_widget, remove_button, Setting, DIAL_R};
use super::knobs::remove_knob;
use super::paint::{full_uv, paint_grille, screw, snapped, spaced, tile, vertical_gradient};
use super::rig;
use super::theme::*;
use super::App;
use crate::models;
use crate::protocol::{self, Kind, BANK_LIVE};

pub(super) const CABINET_PAD: f32 = 16.0;

const SLOT_W: f32 = 102.0;

const SLOT_H: f32 = 138.0;

const PILOT_W: f32 = 84.0;

const PANEL_PAD_Y: f32 = 18.0;

/// Knobs per panel row and the resulting panel height.
pub(super) fn panel_layout(cabinet_width: f32, knobs: &[Knob]) -> (usize, f32) {
    let usable = cabinet_width - 2.0 * CABINET_PAD - PILOT_W - 16.0;
    let columns = ((usable / SLOT_W).floor() as usize).max(1);
    let rows = knobs.len().max(1).div_ceil(columns);
    (columns, rows as f32 * SLOT_H + 2.0 * PANEL_PAD_Y - 12.0)
}

impl App {
    pub(super) fn amp(
        &mut self,
        ui: &mut Ui,
        art: &Art,
        preset: &protocol::Preset,
        width: f32,
        panel_h: f32,
        grille_h: f32,
    ) {
        let Some(block) = preset.block(Kind::Amp) else {
            return;
        };
        let mut frame = *block.as_bytes();
        let before = frame;
        let baseline = self.baseline(preset);
        // A stored slot cannot be written yet, so only the live buffer is editable.
        let editable = !self.connected || preset.bank == BANK_LIVE;

        // The cabinet follows the voice: a '57 Champ is tweed, a '65 Twin is
        // blackface, and a British voice gets its salt-and-pepper grille.
        let model = block
            .model()
            .filter(|model| *model != 0)
            .unwrap_or_else(|| models::voice_model_id(frame[44]));
        let look = ampstyle::for_model(model);
        let surface = art.surface(look.era);
        let name = models::model_name(Kind::Amp, model).unwrap_or("Super Champ X2");

        let height = CABINET_PAD + panel_h + 14.0 + grille_h + CABINET_PAD;
        let (cabinet, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter().clone();

        // cabinet
        painter.add(Shape::mesh(
            Shadow {
                offset: Vec2::new(0.0, 10.0),
                blur: 26.0,
                spread: 0.0,
                color: Color32::from_black_alpha(150),
            }
            .tessellate(cabinet, Rounding::same(16.0)),
        ));
        tile(
            &painter,
            cabinet,
            16.0,
            &surface.tolex,
            art.ppp,
            Color32::WHITE,
        );
        painter.rect_stroke(cabinet, 16.0, stroke(1.0, Color32::from_white_alpha(14)));

        // control panel in a thin chrome trim
        let panel = Rect::from_min_size(
            cabinet.min + Vec2::splat(CABINET_PAD),
            Vec2::new(width - 2.0 * CABINET_PAD, panel_h),
        );
        painter.rect_filled(panel.expand(1.5), 6.0, look.piping.gamma_multiply(0.62));
        painter.line_segment(
            [
                panel.left_top() + Vec2::new(4.0, -1.0),
                panel.right_top() + Vec2::new(-4.0, -1.0),
            ],
            stroke(1.0, look.piping),
        );
        tile(
            &painter,
            panel,
            5.0,
            &surface.panel,
            art.ppp,
            Color32::WHITE,
        );
        let sheen = Rect::from_min_size(panel.min, Vec2::new(panel.width(), 44.0))
            .shrink2(Vec2::new(1.0, 0.0));
        vertical_gradient(
            &painter,
            sheen,
            Color32::from_white_alpha(10),
            Color32::TRANSPARENT,
        );
        for corner in [
            panel.left_top() + Vec2::new(11.0, 11.0),
            panel.right_top() + Vec2::new(-11.0, 11.0),
            panel.left_bottom() + Vec2::new(11.0, -11.0),
            panel.right_bottom() + Vec2::new(-11.0, -11.0),
        ] {
            screw(&painter, corner);
        }

        self.pilot(ui, &painter, art, &look, panel);

        // knobs
        let (columns, _) = panel_layout(width, &self.knobs);
        let origin = panel.min + Vec2::new(PILOT_W, PANEL_PAD_Y);
        // Knobs share the panel evenly, the way they do on the amp.
        let used = columns.min(self.knobs.len().max(1));
        let spread = (panel.width() - PILOT_W - 28.0) / used as f32;
        let slot_at = |index: usize| {
            let (row, column) = (index / columns, index % columns);
            Rect::from_min_size(
                origin + Vec2::new(column as f32 * spread, row as f32 * SLOT_H),
                Vec2::new(spread, SLOT_H),
            )
        };

        let mut remove = None;
        let mut picker_open = false;
        let mut knob_anchor: Option<(egui::Response, egui::AboveOrBelow)> = None;
        let mut plate_anchor: Option<(egui::Response, egui::AboveOrBelow)> = None;
        let knobs = self.knobs.clone();
        let mut placed = 0;
        for knob in &knobs {
            if knob.offset >= protocol::REPORT_SIZE {
                continue;
            }
            let slot = slot_at(placed);
            placed += 1;
            let response = knob_widget(
                ui,
                &painter,
                art,
                &look,
                slot,
                knob,
                Setting {
                    value: &mut frame[knob.offset],
                    baseline: baseline[knob.offset],
                    editable,
                },
            );
            if response.dragged() {
                self.active_offset = Some(knob.offset);
            }
            // The voice knob doubles as a picker: a plain click opens the
            // amp and speaker chooser, a drag still sweeps through the voices.
            if knob.offset == 44 && editable {
                if response.clicked() {
                    picker_open = true;
                    self.picker_above = false;
                }
                knob_anchor = Some((response.clone(), egui::AboveOrBelow::Below));
            }
            response.context_menu(|ui| {
                if ui.button("Remove from Panel").clicked() {
                    remove = Some(knob.offset);
                    ui.close_menu();
                }
            });
            if ui.rect_contains_pointer(slot)
                && remove_button(ui, &painter, slot, knob.offset, look.ink)
            {
                remove = Some(knob.offset);
            }
        }
        if let Some(offset) = remove {
            remove_knob(&mut self.knobs, offset);
        }
        if can_add(&self.knobs) {
            self.add_button(ui, &painter, panel, look.ink);
        }

        // grille cloth
        let grille = Rect::from_min_size(
            Pos2::new(panel.left(), panel.bottom() + 14.0),
            Vec2::new(panel.width(), grille_h),
        );
        let plate = paint_grille(&painter, art, &look, grille, name);

        // The badge on the cloth is the amp's name, so it is also the place to
        // change it.
        let plate = ui.interact(
            plate,
            Id::new("amp-voice"),
            if editable {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if editable {
            let plate = plate
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(format!("{name}\nClick to choose an amp voice and speaker."));
            // A small caret says the plate opens a list.
            let ink = Color32::from_rgb(28, 26, 24);
            let tip = Pos2::new(plate.rect.right() - 14.0, plate.rect.center().y);
            painter.line_segment(
                [tip + Vec2::new(-3.5, -1.5), tip + Vec2::new(0.0, 1.5)],
                stroke(1.2, ink),
            );
            painter.line_segment(
                [tip + Vec2::new(0.0, 1.5), tip + Vec2::new(3.5, -1.5)],
                stroke(1.2, ink),
            );
            if plate.hovered() {
                painter.rect_stroke(plate.rect, 3.0, stroke(1.0, AMBER.gamma_multiply(0.75)));
            }
            if plate.clicked() {
                picker_open = true;
                self.picker_above = true;
            }
            plate_anchor = Some((plate.clone(), egui::AboveOrBelow::Above));
        } else {
            plate.on_hover_text(format!(
                "{name} — choose the Live Buffer to change the voice."
            ));
        }

        let anchor = if self.picker_above {
            plate_anchor.as_ref()
        } else {
            knob_anchor.as_ref()
        };
        let popup = Id::new("amp-picker");
        // Dim and block the editor while the chooser is open. Without this, a
        // click on the popup's edge could reach the effect slot underneath.
        if ui.memory(|memory| memory.is_popup_open(popup)) {
            let screen = ui.ctx().screen_rect();
            let backdrop = egui::Area::new(Id::new("amp-picker-backdrop"))
                .order(egui::Order::Foreground)
                .fixed_pos(screen.min)
                .show(ui.ctx(), |ui| {
                    let response = ui.allocate_rect(screen, Sense::click_and_drag());
                    ui.painter()
                        .rect_filled(screen, 0.0, Color32::from_black_alpha(92));
                    response.clicked()
                });
            if backdrop.inner {
                ui.memory_mut(|memory| memory.close_popup());
            }
        }
        if let Some((anchor, placement)) = anchor {
            if picker_open {
                ui.memory_mut(|memory| memory.toggle_popup(popup));
            }
            let mut keep_open = false;
            egui::popup_above_or_below_widget(ui, popup, anchor, *placement, |ui| {
                ui.set_min_width(430.0);
                ui.set_max_width(470.0);
                ui.spacing_mut().item_spacing.y = 6.0;

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("AMP & SPEAKER")
                            .font(semibold(11.0))
                            .color(TEXT_3),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Done").clicked() {
                            ui.close_menu();
                        }
                    });
                });

                // The picture is the selection: the head wears the voice's era
                // finish and the cab shows the speakers the id stands for.
                let (picture, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), rig::RIG_H),
                    Sense::hover(),
                );
                rig::draw(&ui.painter_at(picture), picture, art, frame[44], frame[49]);

                ui.columns(2, |columns| {
                    columns[0].label(
                        egui::RichText::new("AMP VOICE")
                            .font(semibold(10.0))
                            .color(TEXT_3),
                    );
                    egui::ScrollArea::vertical()
                        .id_source("amp-picker-voices")
                        .max_height(180.0)
                        .auto_shrink([false, true])
                        .show(&mut columns[0], |ui| {
                            ui.set_min_width(190.0);
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for (index, _, voice) in models::voices() {
                                let item = ui.selectable_label(index == frame[44], voice);
                                if index == frame[44] && picker_open {
                                    item.scroll_to_me(Some(egui::Align::Center));
                                }
                                if item.clicked() {
                                    frame[44] = index;
                                    keep_open = true;
                                    ui.ctx().request_repaint();
                                }
                            }
                        });

                    columns[1].label(
                        egui::RichText::new("SPEAKER")
                            .font(semibold(10.0))
                            .color(TEXT_3),
                    );
                    egui::ScrollArea::vertical()
                        .id_source("amp-picker-cabinets")
                        .max_height(180.0)
                        .auto_shrink([false, true])
                        .show(&mut columns[1], |ui| {
                            ui.set_min_width(190.0);
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for (id, name) in models::CABINET_MODELS {
                                let item = ui.selectable_label(*id == frame[49], *name);
                                if *id == frame[49] && picker_open {
                                    item.scroll_to_me(Some(egui::Align::Center));
                                }
                                if item.clicked() {
                                    frame[49] = *id;
                                    keep_open = true;
                                    ui.ctx().request_repaint();
                                }
                            }
                        });
                });
            });
            // A pick closes the popup by way of the anchor's "clicked
            // elsewhere", but this chooser is for two things at once: keep it
            // open so a voice and a speaker can be picked in one visit.
            if keep_open {
                ui.memory_mut(|memory| memory.open_popup(popup));
            }
        }

        self.commit(&frame, &before);
        if frame[44] != before[44] {
            self.status = format!(
                "Voice: {}",
                models::model_name(Kind::Amp, models::voice_model_id(frame[44]))
                    .unwrap_or("Unknown")
            );
        }
        if frame[49] != before[49] {
            self.status = format!(
                "Speaker: {}",
                models::cabinet_name(frame[49]).unwrap_or("custom setting")
            );
        }
    }

    /// Apply whatever the widgets changed in this frame's copy of the block.
    pub(super) fn commit(
        &mut self,
        frame: &[u8; protocol::REPORT_SIZE],
        before: &[u8; protocol::REPORT_SIZE],
    ) {
        for (offset, value) in frame.iter().enumerate() {
            if *value != before[offset] {
                self.apply_edit(Kind::Amp, offset, *value);
            }
        }
    }

    /// The amp block as last read from the amp, which edits are compared to.
    pub(super) fn baseline(&self, preset: &protocol::Preset) -> [u8; protocol::REPORT_SIZE] {
        self.original
            .get(preset.bank, preset.index)
            .and_then(|known| known.block(Kind::Amp))
            .or(preset.block(Kind::Amp))
            .map(|block| *block.as_bytes())
            .unwrap_or([0; protocol::REPORT_SIZE])
    }

    /// The jewel is the connection indicator, exactly as it is on the amp:
    /// lit means alive.
    fn pilot(&self, ui: &mut Ui, painter: &Painter, art: &Art, look: &Look, panel: Rect) {
        let center = Pos2::new(
            panel.left() + PILOT_W / 2.0 + 4.0,
            panel.top() + PANEL_PAD_Y + 20.0 + DIAL_R,
        );
        let level = if self.connected {
            1.0
        } else if self.connecting {
            let time = ui.input(|i| i.time) as f32;
            0.35 + 0.35 * (time * 3.5).sin().abs()
        } else {
            0.0
        };

        spaced(
            painter,
            Pos2::new(center.x, panel.top() + PANEL_PAD_Y + 2.0),
            Align2::CENTER_TOP,
            "POWER",
            semibold(10.5),
            look.ink,
            1.4,
        );

        let shade = (70.0 + level * 185.0) as u8;
        let rect = snapped(
            Rect::from_center_size(center, Vec2::splat(JEWEL_D)),
            art.ppp,
        );
        painter.image(
            art.glow.id(),
            rect.expand(5.0),
            full_uv(),
            Color32::from_black_alpha(190),
        );
        painter.image(
            art.jewel(look.jewel).id(),
            rect,
            full_uv(),
            Color32::from_gray(shade),
        );
        if level > 0.0 {
            painter.image(
                art.glow.id(),
                rect.expand(26.0),
                full_uv(),
                look.accent.gamma_multiply(level * 0.45),
            );
        }

        let tip = if self.connected {
            "Connected to the amp"
        } else if self.connecting {
            "Connecting to the amp"
        } else {
            "Amp not connected"
        };
        ui.interact(rect.expand(6.0), Id::new("pilot"), Sense::hover())
            .on_hover_text(tip);
    }
}
