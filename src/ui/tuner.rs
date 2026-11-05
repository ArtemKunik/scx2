//! The tuner panel: a polyphonic string row, a strobe and the note readout.

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, Layout, Painter, Pos2, Rect, Sense, Ui, Vec2,
};
use std::time::Duration;

use super::paint::elided;
use super::theme::*;
use super::App;
use crate::endpoint::EnableJob;
use crate::tuner::{is_amp_input, Status, StringReading, STRINGS};

const IN_TUNE: Color32 = Color32::from_rgb(126, 207, 143);
const IN_TUNE_DIM: Color32 = Color32::from_rgb(32, 52, 40);
/// A string counts as in tune inside this many cents.
const IN_TUNE_CENTS: f32 = 4.0;
const STRING_H: f32 = 62.0;
const DISPLAY_H: f32 = 96.0;
const GAP: f32 = 6.0;

impl App {
    pub(super) fn tuner_panel(&mut self, ui: &mut Ui, width: f32) {
        // A tuner is a live instrument: keep repainting while it listens, or
        // egui goes idle and the display only moves when the mouse does. The
        // pitch analysis lands about 16 times a second, so 30 fps is plenty.
        if self.tuner.is_active() || self.amp_job.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }
        egui::Frame::none()
            .fill(SIDEBAR)
            .rounding(10.0)
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(width - 24.0);
                let action = self.tuner_header(ui);
                self.tuner_message(ui);
                ui.add_space(8.0);
                self.tuner_strings(ui);
                ui.add_space(12.0);
                self.tuner_display(ui);
                if action.is_some() {
                    self.tuner.start(action.flatten());
                }
            });
    }

    /// Returns `Some(Some(device))` to open a device, `Some(None)` for auto.
    fn tuner_header(&mut self, ui: &mut Ui) -> Option<Option<String>> {
        let status = self.tuner.status;
        let selected = self.tuner.selected.clone();
        let devices = self.tuner.devices.clone();
        let using_amp = selected.as_deref().is_some_and(is_amp_input);
        let mut chosen = None;
        let mut rescan = false;
        let mut enable = false;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("TUNER")
                    .font(semibold(11.0))
                    .color(TEXT_3),
            );
            let (label, color) = match status {
                Status::Running => {
                    if using_amp {
                        ("AMP USB", TEXT_2)
                    } else {
                        ("INPUT", TEXT_2)
                    }
                }
                Status::Starting => ("STARTING…", TEXT_3),
                Status::Error => ("AUDIO ERROR", ERROR),
                Status::Idle => ("IDLE", TEXT_3),
            };
            ui.add_space(4.0);
            ui.label(egui::RichText::new(label).font(semibold(10.0)).color(color));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button("Rescan")
                    .on_hover_text("Look for the amp input again")
                    .clicked()
                {
                    rescan = true;
                }
                let text = selected.as_deref().unwrap_or("Auto (amp USB)");
                egui::ComboBox::from_id_source("tuner-device")
                    .selected_text(egui::RichText::new(text).font(regular(11.0)).color(TEXT_2))
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(selected.is_none(), "Auto (amp USB)")
                            .clicked()
                        {
                            chosen = Some(None);
                        }
                        for name in &devices {
                            if ui
                                .selectable_label(selected.as_deref() == Some(name.as_str()), name)
                                .clicked()
                            {
                                chosen = Some(Some(name.clone()));
                            }
                        }
                        if devices.is_empty() {
                            ui.label(
                                egui::RichText::new("No audio inputs found")
                                    .font(regular(11.0))
                                    .color(TEXT_3),
                            );
                        }
                    });
                if cfg!(windows)
                    && status == Status::Running
                    && !using_amp
                    && ui
                        .small_button("Enable amp input")
                        .on_hover_text("Switch the amp's USB input on in Windows")
                        .clicked()
                {
                    enable = true;
                }
            });
        });

        if enable {
            self.start_amp_input();
        }
        if rescan {
            return Some(None);
        }
        chosen
    }

    /// The one line under the header: what we listen to, or what went wrong.
    fn tuner_message(&mut self, ui: &mut Ui) {
        let message = if let Some(note) = &self.amp_note {
            Some((note.clone(), TEXT_2))
        } else if let Some(error) = &self.tuner.error {
            Some((error.clone(), ERROR))
        } else if self.tuner.status == Status::Running {
            if self.tuner.using_amp() {
                Some((
                    "Amp USB output. Play a string, or strum all six.".to_string(),
                    TEXT_3,
                ))
            } else {
                Some((
                    "Using the default input — enable the amp's USB input above to tune through the amp."
                        .to_string(),
                    TEXT_3,
                ))
            }
        } else {
            None
        };

        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
        if let Some((text, color)) = message {
            let galley = elided(ui.painter(), &text, regular(10.5), color, rect.width());
            ui.painter()
                .galley(Pos2::new(rect.left(), rect.top() + 2.0), galley, color);
        }
    }

    fn tuner_strings(&mut self, ui: &mut Ui) {
        let width = ui.available_width();
        let cell = ((width - GAP * 5.0) / 6.0).max(60.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, STRING_H), Sense::hover());
        let painter = ui.painter();
        for (index, (name, _)) in STRINGS.iter().enumerate() {
            let left = rect.left() + index as f32 * (cell + GAP);
            let cell_rect =
                Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(cell, STRING_H));
            paint_string(painter, cell_rect, name, self.tuner.strings[index]);
        }
    }

    fn tuner_display(&mut self, ui: &mut Ui) {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), DISPLAY_H), Sense::hover());
        let painter = ui.painter();
        let note = self.tuner.note();
        let frequency = self.tuner.frequency();
        let level = self.tuner.reading.level;
        let time = ui.input(|input| input.time) as f32;

        self.tuner_note(painter, rect, note, frequency);
        self.tuner_strobe(painter, rect, note.map(|note| note.cents), time);
        self.tuner_meter(painter, rect, level, note.is_some());

        response.on_hover_cursor(CursorIcon::Default);
    }

    fn tuner_note(
        &self,
        painter: &Painter,
        rect: Rect,
        note: Option<crate::tuner::Note>,
        frequency: Option<f32>,
    ) {
        let left = rect.left() + 6.0;
        let middle = rect.top() + 36.0;

        if let Some(note) = note {
            let in_tune = note.cents.abs() <= IN_TUNE_CENTS;
            let ink = if in_tune { IN_TUNE } else { TEXT };
            painter.text(
                Pos2::new(left, middle),
                Align2::LEFT_CENTER,
                note.name,
                semibold(42.0),
                ink,
            );
            painter.text(
                Pos2::new(left + 42.0, middle + 12.0),
                Align2::LEFT_CENTER,
                note.octave.to_string(),
                regular(16.0),
                TEXT_2,
            );
            painter.text(
                Pos2::new(left, middle + 30.0),
                Align2::LEFT_CENTER,
                if in_tune {
                    format!("in tune  {:+.0}", note.cents)
                } else {
                    format!("{:+.0} cents", note.cents)
                },
                regular(11.5),
                if in_tune { IN_TUNE } else { TEXT_2 },
            );
            if let Some(frequency) = frequency {
                painter.text(
                    Pos2::new(left, middle + 48.0),
                    Align2::LEFT_CENTER,
                    format!("{frequency:.1} Hz"),
                    regular(10.0),
                    TEXT_3,
                );
            }
        } else {
            painter.text(
                Pos2::new(left, middle),
                Align2::LEFT_CENTER,
                "—",
                semibold(40.0),
                TEXT_3,
            );
            let hint = match self.tuner.status {
                Status::Error => "check the input",
                Status::Starting => "waiting for audio",
                _ => "play a string",
            };
            painter.text(
                Pos2::new(left, middle + 30.0),
                Align2::LEFT_CENTER,
                hint,
                regular(11.0),
                TEXT_3,
            );
        }
    }

    /// A strobe band: stripes drift while the string is off pitch and freeze
    /// when it is in tune, like the rotating pattern of a strobe tuner.
    fn tuner_strobe(&self, painter: &Painter, rect: Rect, cents: Option<f32>, time: f32) {
        let band = Rect::from_min_max(
            Pos2::new(rect.left() + 178.0, rect.top() + 4.0),
            Pos2::new(rect.right() - 6.0, rect.top() + 50.0),
        );
        painter.rect_filled(band, 5.0, WINDOW);
        painter.rect_stroke(band, 5.0, stroke(1.0, HAIRLINE));

        let in_tune = cents.is_some_and(|cents| cents.abs() <= IN_TUNE_CENTS);
        let color = match cents {
            Some(_) if in_tune => IN_TUNE.gamma_multiply(0.9),
            Some(_) => AMBER.gamma_multiply(0.75),
            None => HAIRLINE,
        };
        let offset = match cents {
            Some(cents) if !in_tune => -time * 130.0 * (cents / 50.0),
            _ => 0.0,
        };
        let pitch = 26.0;
        let clipped = painter.with_clip_rect(band);
        let mut x = band.left() + offset.rem_euclid(pitch) - pitch;
        while x < band.right() {
            let stripe = Rect::from_min_max(
                Pos2::new(x, band.top() + 7.0),
                Pos2::new(x + 9.0, band.bottom() - 7.0),
            );
            clipped.rect_filled(stripe, 2.0, color);
            x += pitch;
        }

        let center = band.center().x;
        painter.line_segment(
            [
                Pos2::new(center, band.top()),
                Pos2::new(center, band.bottom()),
            ],
            stroke(2.0, if in_tune { IN_TUNE } else { TEXT_2 }),
        );

        // Cents ruler under the band.
        let ruler = Rect::from_min_max(
            Pos2::new(band.left() + 4.0, band.bottom() + 8.0),
            Pos2::new(band.right() - 4.0, band.bottom() + 20.0),
        );
        let ruler_center = ruler.center().x;
        let half = ruler.width() / 2.0;
        for step in -10..=10 {
            let x = ruler_center + half * step as f32 / 10.0;
            let height = if step == 0 {
                9.0
            } else if step % 5 == 0 {
                6.0
            } else {
                3.0
            };
            painter.line_segment(
                [
                    Pos2::new(x, ruler.center().y - height / 2.0),
                    Pos2::new(x, ruler.center().y + height / 2.0),
                ],
                stroke(1.0, if step == 0 { TEXT_2 } else { HAIRLINE }),
            );
        }
        if let Some(cents) = cents {
            let x = ruler_center + half * (cents / 50.0).clamp(-1.0, 1.0);
            let ink = if in_tune { IN_TUNE } else { AMBER };
            painter.circle_filled(Pos2::new(x, ruler.center().y), 3.5, ink);
        }
    }

    fn tuner_meter(&self, painter: &Painter, rect: Rect, level: f32, playing: bool) {
        let meter = Rect::from_min_max(
            Pos2::new(rect.left() + 178.0, rect.bottom() - 8.0),
            Pos2::new(rect.right() - 6.0, rect.bottom() - 4.0),
        );
        painter.text(
            Pos2::new(meter.left(), meter.top() - 4.0),
            Align2::LEFT_BOTTOM,
            "INPUT",
            semibold(8.0),
            TEXT_3,
        );
        painter.rect_filled(meter, 2.0, HAIRLINE);
        let filled = meter.width() * level.clamp(0.0, 1.0).sqrt();
        if filled > 0.0 {
            let mut fill = meter;
            fill.set_right(meter.left() + filled);
            painter.rect_filled(fill, 2.0, if playing { AMBER } else { TEXT_3 });
        }
    }

    fn start_amp_input(&mut self) {
        if self.amp_job.is_none() {
            self.amp_note = Some("Switching on the amp's USB input…".to_string());
            self.amp_job = Some(EnableJob::start());
        }
    }

    /// Pick up the result of switching the amp input on, if it has finished.
    pub(super) fn poll_amp_input(&mut self) {
        let Some(job) = self.amp_job.as_ref() else {
            return;
        };
        let Some(result) = job.poll() else {
            return;
        };
        self.amp_job = None;
        match result {
            Ok(name) => {
                self.amp_note = Some(format!("Enabled {name}."));
                self.tuner.start(None);
            }
            Err(error) => {
                self.amp_note = Some(format!("Could not enable the amp input: {error:#}"));
            }
        }
    }
}

fn paint_string(painter: &Painter, rect: Rect, name: &str, reading: StringReading) {
    let in_tune = reading.active && reading.cents.abs() <= IN_TUNE_CENTS;
    let (fill, edge) = if !reading.active {
        (WINDOW, HAIRLINE)
    } else if in_tune {
        (IN_TUNE_DIM, IN_TUNE)
    } else {
        (RAISED, AMBER.gamma_multiply(0.5))
    };
    painter.rect_filled(rect, 7.0, fill);
    painter.rect_stroke(rect, 7.0, stroke(1.0, edge));

    let (letter, octave) = name.split_at(1);
    let ink = if reading.active { TEXT } else { TEXT_3 };
    painter.text(
        Pos2::new(rect.left() + 12.0, rect.top() + 11.0),
        Align2::LEFT_TOP,
        letter,
        semibold(20.0),
        ink,
    );
    painter.text(
        Pos2::new(rect.left() + 32.0, rect.top() + 20.0),
        Align2::LEFT_TOP,
        octave,
        regular(10.0),
        TEXT_3,
    );

    if reading.active {
        let ink = if in_tune { IN_TUNE } else { AMBER };
        painter.text(
            Pos2::new(rect.right() - 12.0, rect.top() + 13.0),
            Align2::RIGHT_TOP,
            format!("{:+.0}", reading.cents),
            semibold(13.0),
            ink,
        );
    }

    // A little ruler along the bottom of the capsule.
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + 12.0, rect.bottom() - 17.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 15.0),
    );
    painter.rect_filled(track, 1.0, HAIRLINE);
    let center = track.center().x;
    painter.line_segment(
        [
            Pos2::new(center, track.top() - 3.0),
            Pos2::new(center, track.bottom() + 3.0),
        ],
        stroke(1.0, TEXT_3),
    );
    if reading.active {
        let x = center + (track.width() / 2.0) * (reading.cents / 50.0).clamp(-1.0, 1.0);
        let ink = if in_tune { IN_TUNE } else { AMBER };
        painter.circle_filled(Pos2::new(x, track.center().y), 3.5, ink);
    }
}
