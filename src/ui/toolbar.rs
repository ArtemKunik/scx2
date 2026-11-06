//! The toolbar over the amp and the status line under it.

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, Id, Layout, Pos2, Rect, Sense, Ui, Vec2,
};

use super::paint::elided;
use super::theme::*;
use super::App;
use crate::protocol::{BANK_AMP, BANK_FX, BANK_LIVE};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ButtonKind {
    Plain,
    Primary,
}

pub(super) fn tool_button(
    ui: &mut Ui,
    label: &str,
    enabled: bool,
    kind: ButtonKind,
) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), semibold(12.5), Color32::PLACEHOLDER);
    let size = Vec2::new(galley.size().x + 26.0, 30.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();

    let (fill, ink) = match (kind, enabled) {
        (ButtonKind::Primary, true) => (
            if pressed {
                Color32::from_rgb(226, 152, 56)
            } else if hovered {
                Color32::from_rgb(255, 193, 104)
            } else {
                AMBER
            },
            AMBER_INK,
        ),
        (_, false) => (Color32::from_white_alpha(6), TEXT_3),
        (_, true) => (
            Color32::from_white_alpha(if pressed {
                34
            } else if hovered {
                24
            } else {
                14
            }),
            TEXT,
        ),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 7.0, fill);
    painter.galley(rect.center() - galley.size() / 2.0, galley, ink);
    response
}

impl App {
    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let key = self.selected;
        let preset = self.dump.get(key.0, key.1);
        let mut title = match preset {
            Some(_) if key.0 == BANK_LIVE => "Live Buffer".to_string(),
            Some(preset) if !preset.name.is_empty() => preset.name.clone(),
            Some(preset) => format!("Preset {}", preset.index),
            None => "No preset selected".to_string(),
        };
        let dirty = self.dirty();
        if dirty {
            title.insert_str(0, "● ");
        }
        let saved = if self.connected {
            ". Stored on the amp, read-only here"
        } else {
            ""
        };
        let place = if !self.connected {
            format!("Not connected — {}", self.source)
        } else {
            match key.0 {
                BANK_LIVE => {
                    if dirty {
                        "What the amp is playing now · unsaved edits".to_string()
                    } else {
                        "What the amp is playing now".to_string()
                    }
                }
                BANK_AMP => format!("Amp preset {}{saved}", key.1),
                BANK_FX => format!("FX preset {}{saved}", key.1),
                _ => String::new(),
            }
        };

        ui.horizontal_centered(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(egui::RichText::new(title).font(semibold(20.0)).color(TEXT));
                ui.label(
                    egui::RichText::new(place)
                        .font(regular(11.5))
                        .color(if self.connected { TEXT_2 } else { ERROR }),
                );
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if tool_button(ui, "Save As…", true, ButtonKind::Primary).clicked() {
                    self.save_dialog();
                }
                if tool_button(ui, "Save to Amp…", true, ButtonKind::Plain).clicked() {
                    self.open_store_dialog();
                }
                if tool_button(ui, "Open…", true, ButtonKind::Plain).clicked() {
                    self.open_dialog();
                }
                if tool_button(ui, "Save Backup", true, ButtonKind::Plain).clicked() {
                    self.save_backup();
                }
                if tool_button(ui, "Restore…", true, ButtonKind::Plain).clicked() {
                    self.open_restore_dialog();
                }
                if tool_button(ui, "Undo", !self.history.is_empty(), ButtonKind::Plain).clicked() {
                    self.undo();
                }
                if tool_button(ui, "Redo", !self.future.is_empty(), ButtonKind::Plain).clicked() {
                    self.redo();
                }
                // Without an amp, edits are made to a backup and can be undone.
                if !self.connected
                    && !self.pending().is_empty()
                    && tool_button(ui, "Revert", true, ButtonKind::Plain).clicked()
                {
                    self.begin_edit();
                    self.dump = self.original.clone();
                    self.pending_live.clear();
                    self.status = "Changes reverted".into();
                }
                if tool_button(
                    ui,
                    if self.show_tuner {
                        "Hide Tuner"
                    } else {
                        "Tuner"
                    },
                    true,
                    ButtonKind::Plain,
                )
                .clicked()
                {
                    self.show_tuner = !self.show_tuner;
                    if self.show_tuner {
                        self.tuner.start(None);
                    } else {
                        self.tuner.stop();
                        self.amp_job = None;
                        self.amp_note = None;
                    }
                }
                if tool_button(
                    ui,
                    if self.show_recorder {
                        "Hide Recorder"
                    } else {
                        "Recorder"
                    },
                    true,
                    ButtonKind::Plain,
                )
                .clicked()
                {
                    self.show_recorder = !self.show_recorder;
                    if !self.show_recorder {
                        self.recorder.stop();
                    }
                }
                if tool_button(ui, "MIDI", true, ButtonKind::Plain)
                    .on_hover_text("MIDI foot controller for effect on/off")
                    .clicked()
                {
                    self.open_midi_window();
                }
            });
        });
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        let y = rect.center().y;

        let Some(error) = self.error.clone() else {
            painter.text(
                Pos2::new(rect.right(), y),
                Align2::RIGHT_CENTER,
                "Drop a backup or .fuse preset anywhere to open it",
                regular(11.5),
                TEXT_3,
            );
            let galley = elided(
                &painter,
                &self.status,
                regular(11.5),
                TEXT_3,
                rect.width() * 0.6,
            );
            let at = Pos2::new(rect.left(), y - galley.size().y / 2.0);
            painter.galley(at, galley, TEXT_3);
            return;
        };

        let cross = Pos2::new(rect.left() + 6.0, y);
        painter.text(cross, Align2::CENTER_CENTER, "×", regular(14.0), ERROR);
        let dismiss = ui
            .interact(
                Rect::from_center_size(cross, Vec2::splat(18.0)),
                Id::new("dismiss-error"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("Dismiss");

        // First line here; troubleshooting hints live in the tooltip.
        let headline = error.lines().next().unwrap_or_default();
        let galley = elided(
            &painter,
            headline,
            regular(11.5),
            ERROR,
            rect.width() - 20.0,
        );
        let area = Rect::from_min_size(
            Pos2::new(rect.left() + 18.0, y - galley.size().y / 2.0),
            galley.size(),
        );
        painter.galley(area.min, galley, ERROR);
        ui.interact(area, Id::new("error-text"), Sense::hover())
            .on_hover_text(error.clone());

        if dismiss.clicked() {
            self.error = None;
        }
    }
}
