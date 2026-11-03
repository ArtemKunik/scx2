//! The amp card: real knobs, the optional hidden controls and the parameter grid.

use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Ui, Vec2};

use crate::models;
use crate::protocol::{self, Kind};

use super::faceplate::{effect_card, paint_faceplate};
use super::knobs::knob_menu;
use super::knobs::{add_knob, remove_knob};
use super::widgets::{knob_range, knob_value_label, knob_widget, param_cell};
use super::worker::Cmd;
use super::{bank_name, stroke, App, ACCENT, BAD, EDITED, MUTED, PANEL, TEXT, TOLEX};
impl App {
    pub(crate) fn body(&mut self, ui: &mut Ui) {
        let key = self.selected;
        let Some(preset) = self.dump.get(key.0, key.1).cloned() else {
            ui.label("select a preset");
            return;
        };

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                if let Some(tx) = &self.tx {
                    let _ = tx.send(Cmd::Refresh);
                    self.status = "requesting dump...".into();
                }
            }
            if ui.button("Save backup").clicked() {
                self.save_backup();
            }
            if ui.button("Load...").clicked() {
                self.status = "drop a .json onto the window".into();
            }
            ui.label(
                egui::RichText::new("drop a backup on the window to open it")
                    .font(FontId::proportional(10.0))
                    .color(MUTED),
            );
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if preset.name.is_empty() {
                    "Live buffer".to_string()
                } else {
                    preset.name.clone()
                })
                .font(FontId::proportional(22.0))
                .color(TEXT)
                .strong(),
            );
            ui.label(
                egui::RichText::new(format!("bank {} - slot {}", bank_name(key.0), key.1))
                    .font(FontId::proportional(11.0))
                    .color(MUTED),
            );
        });
        ui.add_space(10.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.amp_card(ui, &preset);
                ui.add_space(16.0);
                self.effects(ui, &preset);
                if self.polling || !self.learn_log.is_empty() {
                    ui.add_space(16.0);
                    self.learn_panel(ui);
                }
            });
    }

    pub(crate) fn amp_card(&mut self, ui: &mut Ui, preset: &protocol::Preset) {
        let Some(amp) = preset.block(Kind::Amp) else {
            return;
        };
        let mut frame = *amp.as_bytes();
        let original = frame;
        let model = amp.model().unwrap_or(0);
        let voice = models::model_name(Kind::Amp, model).unwrap_or("unknown voice");
        let connected = self.connected;

        egui::Frame::none()
            .fill(TOLEX)
            .rounding(16.0)
            .stroke(stroke(1.0, Color32::from_rgb(44, 38, 33)))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                let (plate, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 62.0), Sense::hover());
                paint_faceplate(ui, plate, voice, connected);
                ui.add_space(14.0);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("CONTROLS")
                            .font(FontId::proportional(9.5))
                            .color(Color32::from_rgb(120, 124, 134))
                            .strong(),
                    );
                    ui.add_space(6.0);
                    knob_menu(ui, &mut self.knobs);
                    ui.label(
                        egui::RichText::new("front-panel knobs are here, hidden ones are guesses")
                            .font(FontId::proportional(9.5))
                            .color(Color32::from_rgb(96, 100, 110)),
                    );
                });
                ui.add_space(2.0);

                ui.horizontal_wrapped(|ui| {
                    let knobs = self.knobs.clone();
                    let mut remove: Option<usize> = None;
                    for knob in &knobs {
                        let offset = knob.offset;
                        if offset >= protocol::REPORT_SIZE {
                            continue;
                        }
                        let sub = knob_value_label(knob.offset, frame[offset]);
                        let range = knob_range(knob.offset);
                        let (response, rect) = knob_widget(
                            ui,
                            &mut frame[offset],
                            &knob.label,
                            &sub,
                            range,
                            if knob.offset == 44 { ACCENT } else { EDITED },
                        );

                        // visible remove affordance, plus a right-click menu
                        let close_rect = Rect::from_center_size(
                            rect.right_top() + Vec2::new(-9.0, 9.0),
                            Vec2::splat(18.0),
                        );
                        let close = ui.interact(
                            close_rect,
                            ui.id().with(("knob-x", knob.offset)),
                            Sense::click(),
                        );
                        let painter = ui.painter();
                        painter.circle_filled(
                            close_rect.center(),
                            8.0,
                            if close.hovered() {
                                BAD
                            } else {
                                Color32::from_rgb(56, 60, 70)
                            },
                        );
                        painter.text(
                            close_rect.center(),
                            Align2::CENTER_CENTER,
                            "x",
                            FontId::proportional(10.0),
                            TEXT,
                        );
                        if close.clicked() {
                            remove = Some(knob.offset);
                        }
                        response.context_menu(|ui| {
                            if ui.button("Remove this knob").clicked() {
                                remove = Some(knob.offset);
                                ui.close_menu();
                            }
                        });
                    }
                    if let Some(offset) = remove {
                        remove_knob(&mut self.knobs, offset);
                    }
                });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "drag up/down to change - the x on a knob removes it - \
                         right-click a parameter cell to add it as a knob",
                    )
                    .font(FontId::proportional(10.0))
                    .color(Color32::from_rgb(120, 124, 134)),
                );

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("PARAMETERS")
                            .font(FontId::proportional(9.5))
                            .color(Color32::from_rgb(120, 124, 134))
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("32..63 - 0x80 is centre")
                            .font(FontId::proportional(9.5))
                            .color(Color32::from_rgb(96, 100, 110)),
                    );
                });
                ui.add_space(6.0);

                let mut bind_request = None;
                let columns = (((ui.available_width() + 8.0) / 60.0).floor() as usize).clamp(4, 8);
                for row_start in (0..32).step_by(columns) {
                    ui.horizontal(|ui| {
                        for index in row_start..(row_start + columns).min(32) {
                            let offset = protocol::OFF_PARAMS + index;
                            let flash = self.flash.get(&offset).copied().unwrap_or(0.0);
                            let response =
                                param_cell(ui, offset, &mut frame[offset], original[offset], flash);
                            if response.secondary_clicked() {
                                bind_request = Some(offset);
                            }
                        }
                    });
                }
                if let Some(offset) = bind_request {
                    self.add_knob_at(offset);
                }
            });

        if frame != original {
            for (offset, value) in frame.iter().enumerate() {
                if *value != original[offset] {
                    self.apply_edit(Kind::Amp, offset, *value);
                }
            }
        }
    }

    pub(crate) fn add_knob_at(&mut self, offset: usize) {
        add_knob(&mut self.knobs, offset);
    }

    pub(crate) fn effects(&mut self, ui: &mut Ui, preset: &protocol::Preset) {
        ui.label(
            egui::RichText::new("EFFECTS")
                .font(FontId::proportional(10.0))
                .color(ACCENT)
                .strong(),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (kind, title) in [
                (Kind::Stomp, "STOMP"),
                (Kind::Mod, "MOD"),
                (Kind::Delay, "DELAY"),
                (Kind::Reverb, "REVERB"),
            ] {
                effect_card(ui, title, preset, kind);
            }
        });
    }

    pub(crate) fn learn_panel(&mut self, ui: &mut Ui) {
        egui::Frame::none()
            .fill(PANEL)
            .rounding(14.0)
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("LEARN")
                        .font(FontId::proportional(10.0))
                        .color(ACCENT)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "Turn one knob on the amp. The offsets that moved are listed here and \
                         flash in the parameter grid. Right-click a cell to bind it to a knob.",
                    )
                    .font(FontId::proportional(11.0))
                    .color(MUTED),
                );
                if self.learn_log.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("waiting for a change...")
                            .font(FontId::proportional(11.0))
                            .color(MUTED),
                    );
                }
                for line in &self.learn_log {
                    ui.label(
                        egui::RichText::new(line)
                            .font(FontId::monospace(11.0))
                            .color(TEXT),
                    );
                }
            });
    }
}
