//! "Rename…": change a stored slot's name without rewriting its sound.
//!
//! FUSE renames by writing the `0x04` name block and an execute frame; the
//! blocks that carry the sound are not part of the write. The worker reads the
//! slot back and compares the payload with what it saw before the rename.

use std::time::Instant;

use eframe::egui::{self, Align, Color32, Layout, RichText};

use super::name::{fit_name, fits, name_input};
use super::theme::*;
use super::worker::Cmd;
use super::App;
use crate::protocol::{self, BANK_AMP, BANK_FX};

#[derive(Clone)]
pub(crate) struct RenameDialog {
    bank: u8,
    index: u8,
    name: String,
}

impl App {
    /// Open the dialog for a stored slot, from its sidebar context menu.
    pub(super) fn open_rename_dialog(&mut self, bank: u8, index: u8) {
        if bank != BANK_AMP && bank != BANK_FX {
            return;
        }
        if self.amp_busy() || self.dialog_open() {
            self.status = "Another amp operation is still running; try again in a moment.".into();
            return;
        }
        let Some(preset) = self.dump.get(bank, index) else {
            return;
        };
        if preset.name.is_empty() {
            self.status = "That slot has no name to change.".into();
            return;
        }
        self.rename = Some(RenameDialog {
            bank,
            index,
            name: preset.name.clone(),
        });
    }

    pub(super) fn rename_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.rename.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;

        let current = self
            .dump
            .get(dialog.bank, dialog.index)
            .map(|preset| preset.name.clone())
            .unwrap_or_default();

        egui::Window::new("Rename Preset")
            .id(egui::Id::new("rename-preset"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(
                    RichText::new(format!(
                        "{} {} · now {:?}",
                        bank_name(dialog.bank),
                        dialog.index,
                        current
                    ))
                    .font(regular(11.5))
                    .color(TEXT_3),
                );
                ui.label(
                    RichText::new("Writes only the name block; the sound stays untouched.")
                        .font(regular(11.5))
                        .color(TEXT_2),
                );
                ui.add_space(10.0);
                name_input(ui, &mut dialog.name);
                ui.add_space(10.0);

                if !self.connected {
                    ui.label(
                        RichText::new(
                            "Not connected — the frames are printed to the status bar instead.",
                        )
                        .font(regular(11.5))
                        .color(TEXT_2),
                    );
                    ui.add_space(4.0);
                } else if !self.synced {
                    ui.label(
                        RichText::new("Waiting for the first dump…")
                            .font(regular(11.5))
                            .color(TEXT_2),
                    );
                    ui.add_space(4.0);
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ready = !dialog.name.trim().is_empty()
                        && fits(&dialog.name)
                        && (self.synced || !self.connected);
                    let rename = egui::Button::new(
                        RichText::new("Rename")
                            .font(semibold(13.0))
                            .color(if ready { AMBER_INK } else { TEXT_3 }),
                    )
                    .fill(if ready {
                        AMBER
                    } else {
                        Color32::from_white_alpha(8)
                    });
                    if ui.add_enabled(ready, rename).clicked() {
                        confirm = true;
                    }
                    if ui
                        .add(egui::Button::new(
                            RichText::new("Cancel").font(regular(12.5)),
                        ))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });

        if confirm {
            if self.commit_rename(&mut dialog) {
                self.rename = None;
            } else {
                self.rename = Some(dialog);
            }
            return;
        }
        if cancel || !open {
            self.rename = None;
            return;
        }
        self.rename = Some(dialog);
    }

    /// Send the name block and the execute frame, or print them without an amp.
    pub(super) fn commit_rename(&mut self, dialog: &mut RenameDialog) -> bool {
        let name = fit_name(dialog.name.trim());
        if name.is_empty() {
            self.error = Some("The amp needs a name; type at least one character.".into());
            return false;
        }
        let raw = protocol::name_change(dialog.bank, dialog.index, &name);
        let frame = protocol::as_host_frame(&raw, protocol::HOST_CONTROL);
        if !self.connected {
            self.status = format!(
                "Dry run: {} + {} — connect the amp to write it",
                protocol::hex(&frame),
                protocol::hex(&protocol::execute_model_change())
            );
            return true;
        }
        let Some(tx) = self.tx.as_ref() else {
            self.error = Some("The amp worker is not running.".into());
            return false;
        };
        let command = Cmd::Rename {
            bank: dialog.bank,
            index: dialog.index,
            name: name.clone(),
        };
        if let Err(error) = tx.send(command) {
            self.error = Some(format!("Could not reach the amp worker: {error}"));
            return false;
        }
        self.waiting = Some((Instant::now(), "rename"));
        self.status = format!(
            "Renaming {} {} to {:?}…",
            protocol::bank_label(dialog.bank),
            dialog.index,
            name
        );
        true
    }
}

fn bank_name(bank: u8) -> &'static str {
    match bank {
        BANK_FX => "FX preset",
        _ => "Amp preset",
    }
}
