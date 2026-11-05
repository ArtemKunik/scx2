//! The take recorder panel: record the amp's USB output to a WAV.
//!
//! The amp's USB audio is a finished sound (post-effects, post-cabinet), not a
//! dry DI. A sidecar JSON with the live preset is written next to every take so
//! the sound can be recalled.

use std::time::Duration;

use eframe::egui::{self, Align, Align2, Color32, Layout, Pos2, Rect, RichText, Sense, Ui, Vec2};

use super::paint::elided;
use super::theme::*;
use super::App;
use crate::protocol::{Kind, BANK_LIVE};
use crate::recorder::{self, Status};

const REC: Color32 = Color32::from_rgb(226, 92, 82);
const METER_H: f32 = 9.0;

impl App {
    pub(super) fn recorder_panel(&mut self, ui: &mut Ui, width: f32) {
        if self.recorder.is_recording() {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
        if !self.recorder.scanned && !self.recorder.is_recording() {
            self.recorder.scan();
        }

        egui::Frame::none()
            .fill(SIDEBAR)
            .rounding(10.0)
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(width - 24.0);
                self.recorder_header(ui);
                self.recorder_message(ui);
                ui.add_space(8.0);
                self.recorder_actions(ui);
                ui.add_space(7.0);
                self.recorder_meter(ui);
                self.recorder_note(ui);
            });
    }

    fn recorder_header(&mut self, ui: &mut Ui) {
        let status = self.recorder.status;
        let selected = self.recorder.selected.clone();
        let devices = self.recorder.devices.clone();
        let channels = self.recorder.channels();
        let channel = self
            .recorder
            .channel
            .min(channels.saturating_sub(1) as usize);
        let mut chosen = None;
        let mut chosen_channel = None;
        let mut rescan = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new("RECORDER").font(semibold(11.0)).color(TEXT_3));
            let (label, color) = match status {
                Status::Recording => ("● REC", REC),
                Status::Starting => ("STARTING…", TEXT_3),
                Status::Error => ("AUDIO ERROR", ERROR),
                Status::Idle => ("READY", TEXT_3),
            };
            ui.add_space(4.0);
            ui.label(RichText::new(label).font(semibold(10.0)).color(color));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button("Rescan")
                    .on_hover_text("Look for the amp input again")
                    .clicked()
                {
                    rescan = true;
                }
                let text = selected.as_deref().unwrap_or("Auto (amp USB)");
                egui::ComboBox::from_id_source("recorder-device")
                    .selected_text(RichText::new(text).font(regular(11.0)).color(TEXT_2))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(selected.is_none(), "Auto (amp USB)")
                            .clicked()
                        {
                            chosen = Some(None);
                        }
                        for input in &devices {
                            if ui
                                .selectable_label(
                                    selected.as_deref() == Some(input.name.as_str()),
                                    &input.name,
                                )
                                .clicked()
                            {
                                chosen = Some(Some(input.name.clone()));
                            }
                        }
                        if devices.is_empty() {
                            ui.label(
                                RichText::new("No audio inputs found")
                                    .font(regular(11.0))
                                    .color(TEXT_3),
                            );
                        }
                    });
                if channels > 1 {
                    egui::ComboBox::from_id_source("recorder-channel")
                        .selected_text(
                            RichText::new(format!("Ch {}", channel + 1))
                                .font(regular(11.0))
                                .color(TEXT_2),
                        )
                        .width(62.0)
                        .show_ui(ui, |ui| {
                            for index in 0..channels as usize {
                                if ui
                                    .selectable_label(channel == index, format!("Ch {}", index + 1))
                                    .clicked()
                                {
                                    chosen_channel = Some(index);
                                }
                            }
                        });
                }
            });
        });

        if rescan {
            self.recorder.scan();
        }
        if let Some(device) = chosen {
            self.recorder.selected = device;
            self.recorder.channel = 0;
        }
        if let Some(channel) = chosen_channel {
            self.recorder.channel = channel;
        }
    }

    fn recorder_message(&mut self, ui: &mut Ui) {
        let message = if let Some(error) = &self.recorder.error {
            Some((error.clone(), ERROR))
        } else if self.recorder.is_recording() {
            let device = self
                .recorder
                .selected
                .as_deref()
                .unwrap_or("the amp / default input");
            Some((format!("Recording from {device}."), TEXT_2))
        } else if let Some(path) = &self.recorder.path {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("take.wav");
            Some((format!("Saved {name} (+ .json)."), TEXT_3))
        } else {
            Some((
                "Writes a 24-bit WAV with the live preset beside it.".to_string(),
                TEXT_3,
            ))
        };

        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
        if let Some((text, color)) = message {
            let galley = elided(ui.painter(), &text, regular(10.5), color, rect.width());
            ui.painter()
                .galley(Pos2::new(rect.left(), rect.top() + 2.0), galley, color);
        }
    }

    fn recorder_actions(&mut self, ui: &mut Ui) {
        let recording = self.recorder.is_recording();
        let seconds = self.recorder.seconds;
        let mut start = false;
        let mut stop = false;
        let mut open = false;

        ui.horizontal(|ui| {
            let label = if recording { "■ Stop" } else { "● Record" };
            let ink = if recording { Color32::WHITE } else { AMBER_INK };
            let fill = if recording { REC } else { AMBER };
            let button = egui::Button::new(RichText::new(label).font(semibold(12.0)).color(ink))
                .fill(fill)
                .rounding(6.0)
                .min_size(Vec2::new(104.0, 28.0));
            if ui.add(button).clicked() {
                if recording {
                    stop = true;
                } else {
                    start = true;
                }
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new(clock(seconds))
                    .font(regular(13.0))
                    .color(if recording { TEXT } else { TEXT_2 }),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button("Open folder")
                    .on_hover_text("Show the takes folder")
                    .clicked()
                {
                    open = true;
                }
            });
        });

        if open {
            recorder::open_recordings();
        }
        if stop {
            self.recorder.stop();
        }
        if start {
            let meta = self.recording_meta();
            let device = self.recorder.selected.clone();
            let channel = self.recorder.channel;
            self.recorder.start(device, channel, meta);
        }
    }

    fn recorder_meter(&self, ui: &mut Ui) {
        let level = self.recorder.level.clamp(0.0, 1.0);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 14.0), Sense::hover());
        let track = Rect::from_min_max(
            rect.left_top(),
            Pos2::new(rect.right(), rect.top() + METER_H),
        );
        ui.painter().rect_filled(track, 2.0, HAIRLINE);
        let filled = track.width() * level.sqrt();
        if filled > 0.0 {
            let mut fill = track;
            fill.set_right(track.left() + filled);
            ui.painter().rect_filled(fill, 2.0, AMBER);
        }
        let db = if level > 1e-6 {
            20.0 * level.log10()
        } else {
            -120.0
        };
        ui.painter().text(
            Pos2::new(rect.right(), track.center().y),
            Align2::RIGHT_CENTER,
            format!("{db:.0} dBFS"),
            regular(10.0),
            TEXT_3,
        );
        ui.painter().text(
            Pos2::new(track.left(), track.center().y),
            Align2::LEFT_CENTER,
            "INPUT",
            semibold(8.0),
            TEXT_3,
        );
    }

    fn recorder_note(&self, ui: &mut Ui) {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
        let text = "For NAM/IR re-amping record the dry guitar from an interface input; amp USB is post-amp.";
        let galley = elided(ui.painter(), text, regular(10.0), TEXT_3, rect.width());
        ui.painter()
            .galley(Pos2::new(rect.left(), rect.top()), galley, TEXT_3);
    }

    /// The preset snapshot written into the take's sidecar.
    fn recording_meta(&self) -> serde_json::Value {
        let live = self.dump.get(BANK_LIVE, 0);
        let amp = live.and_then(|preset| preset.block(Kind::Amp));
        serde_json::json!({
            "app": format!("scx2 {}", env!("CARGO_PKG_VERSION")),
            "selected": { "bank": self.selected.0, "index": self.selected.1 },
            "live": live.map(|preset| preset.to_json()),
            "amp": amp.map(|block| serde_json::json!({
                "voice": block.byte(44),
                "cabinet": block.byte(49),
                "model": block.model(),
            })),
        })
    }
}

fn clock(seconds: f32) -> String {
    let seconds = seconds.max(0.0);
    let minutes = (seconds / 60.0).floor() as u32;
    let rest = seconds - minutes as f32 * 60.0;
    format!("{minutes:02}:{rest:04.1}")
}
