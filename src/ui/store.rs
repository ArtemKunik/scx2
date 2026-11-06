//! "Save to Amp…": FUSE's SAVE dialog, on the verified save-preset frame.
//!
//! The amp copies its live buffer into a slot; there is no per-block payload to
//! send. The dialog therefore shows what is about to be replaced, offers a
//! backup of the target slot, and the worker reads the slot back afterwards.

use std::time::Instant;

use eframe::egui::{self, Align, Color32, Layout, RichText};

use super::name::{fit_name, fits, name_input};
use super::theme::*;
use super::worker::Cmd;
use super::App;
use crate::protocol::{self, Dump, BANK_AMP, BANK_FX};

/// Bank 1 has 17 slots: 0 is the voice slot, 1..=16 are stored presets.
const AMP_SLOTS: u8 = 17;
/// Bank 2 has 16 effect presets.
const FX_SLOTS: u8 = 16;

#[derive(Clone)]
pub(crate) struct StoreDialog {
    bank: u8,
    index: u8,
    name: String,
    /// Write the slot's current contents out before replacing them.
    backup: bool,
    /// Once the person types a name, slot changes stop replacing it.
    name_touched: bool,
}

impl App {
    /// Open the dialog on the current selection, or on the first empty slot.
    pub(super) fn open_store_dialog(&mut self) {
        if self.amp_busy() || self.restore_dialog_open() || self.import.is_some() {
            self.status = "Another amp operation is still running; try again in a moment.".into();
            return;
        }
        let (bank, index) = store_defaults(&self.dump, self.selected);
        let name = self
            .dump
            .get(bank, index)
            .map(|preset| preset.name.clone())
            .unwrap_or_default();
        self.store = Some(StoreDialog {
            bank,
            index,
            name,
            // Replacing a named slot starts with a backup ticked; the box only
            // appears for a slot that has something to lose.
            backup: true,
            name_touched: false,
        });
        // Read the target before offering to replace it.
        if self.connected {
            if let Some(tx) = &self.tx {
                let _ = tx.send(Cmd::Refresh);
            }
        }
    }

    pub(super) fn store_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.store.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;

        // Until the person types, the name follows the slot the amp reports.
        if !dialog.name_touched {
            dialog.name = self
                .dump
                .get(dialog.bank, dialog.index)
                .map(|preset| preset.name.clone())
                .unwrap_or_default();
        }

        egui::Window::new("Save to Amp")
            .id(egui::Id::new("save-to-amp"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(
                    RichText::new("Writes the live buffer into a stored slot.")
                        .font(regular(11.5))
                        .color(TEXT_3),
                );
                ui.add_space(10.0);

                // Name -----------------------------------------------------------------
                if name_input(ui, &mut dialog.name) {
                    dialog.name_touched = true;
                }
                ui.add_space(10.0);

                // Bank and slot --------------------------------------------------------
                ui.label(RichText::new("Slot").font(semibold(12.0)).color(TEXT));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut dialog.bank, BANK_AMP, "Amp presets");
                    ui.selectable_value(&mut dialog.bank, BANK_FX, "FX presets");
                });
                dialog.bank = clamp_bank(dialog.bank);
                let slots = slot_count(dialog.bank);
                if dialog.index >= slots {
                    dialog.index = slots - 1;
                }
                ui.add_space(6.0);
                egui::Grid::new("store-slots")
                    .num_columns(6)
                    .spacing([4.0, 4.0])
                    .show(ui, |ui| {
                        for index in 0..slots {
                            let preset = self.dump.get(dialog.bank, index);
                            let slot_name = match preset.filter(|preset| !preset.name.is_empty()) {
                                Some(preset) => preset.name.clone(),
                                None => "empty".to_string(),
                            };
                            let label = if dialog.bank == BANK_AMP && index == 0 {
                                format!("{index} · voice slot · {slot_name}")
                            } else {
                                format!("{index} · {slot_name}")
                            };
                            let selected = index == dialog.index;
                            let text = RichText::new(format!("{index}")).font(regular(12.0));
                            if ui
                                .add(egui::SelectableLabel::new(selected, text))
                                .on_hover_text(label)
                                .clicked()
                            {
                                dialog.index = index;
                            }
                            if (index + 1) % 6 == 0 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(10.0);

                // What is being replaced ----------------------------------------------
                let occupied = !slot_is_empty(&self.dump, dialog.bank, dialog.index);
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(if occupied { "Replaces" } else { "Empty slot" })
                            .font(semibold(12.0))
                            .color(if occupied { AMBER } else { TEXT_2 }),
                    );
                    if occupied {
                        let current = self
                            .dump
                            .get(dialog.bank, dialog.index)
                            .map(|preset| preset.name.clone())
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(format!(
                                "{} {} — {:?}",
                                bank_name(dialog.bank),
                                dialog.index,
                                if current.is_empty() {
                                    "(no name)"
                                } else {
                                    current.as_str()
                                }
                            ))
                            .font(regular(12.0))
                            .color(TEXT),
                        );
                    }
                });
                if occupied {
                    ui.checkbox(
                        &mut dialog.backup,
                        RichText::new("Back up the current slot to a .fuse file first")
                            .font(regular(11.5)),
                    );
                }
                if dialog.bank == BANK_AMP && dialog.index == 0 {
                    ui.label(
                        RichText::new("Slot 0 is the voice slot; writing it is untested.")
                            .font(regular(11.0))
                            .color(TEXT_3),
                    );
                }
                if let Some(at) = self.synced_at {
                    ui.label(
                        RichText::new(format!(
                            "Slot contents read {} s ago",
                            at.elapsed().as_secs()
                        ))
                        .font(regular(11.0))
                        .color(TEXT_3),
                    );
                }

                if !self.connected {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Not connected — the frame is printed to the status bar instead.",
                        )
                        .font(regular(11.5))
                        .color(TEXT_2),
                    );
                } else if !self.synced {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Waiting for the first dump…")
                            .font(regular(11.5))
                            .color(TEXT_2),
                    );
                }

                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ready = fits(&dialog.name) && (self.synced || !self.connected);
                    let save = egui::Button::new(
                        RichText::new("Save to Amp")
                            .font(semibold(13.0))
                            .color(if ready { AMBER_INK } else { TEXT_3 }),
                    )
                    .fill(if ready {
                        AMBER
                    } else {
                        Color32::from_white_alpha(8)
                    });
                    if ui.add_enabled(ready, save).clicked() {
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
            if self.commit_store(&dialog) {
                self.store = None;
            } else {
                self.store = Some(dialog);
            }
            return;
        }
        if cancel || !open {
            self.store = None;
            return;
        }
        // Keep the "read N s ago" line honest while the dialog is idle.
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        self.store = Some(dialog);
    }

    /// Send the save frame, or print it when there is no amp.
    ///
    /// Returns false when the save did not happen and the dialog should stay
    /// open, so a cancelled backup does not silently overwrite the slot.
    pub(super) fn commit_store(&mut self, dialog: &StoreDialog) -> bool {
        let name = fit_name(dialog.name.trim());
        if !self.connected {
            let frame = protocol::save_preset(dialog.bank, dialog.index, &name);
            self.status = format!(
                "Dry run: {} — connect the amp to write it",
                protocol::hex(&frame)
            );
            return true;
        }
        if dialog.backup
            && !slot_is_empty(&self.dump, dialog.bank, dialog.index)
            && !self.backup_slot(dialog.bank, dialog.index)
        {
            return false;
        }
        let Some(tx) = self.tx.as_ref() else {
            self.error = Some("The amp worker is not running.".into());
            return false;
        };
        let command = Cmd::Store {
            bank: dialog.bank,
            index: dialog.index,
            name: name.clone(),
        };
        if let Err(error) = tx.send(command) {
            self.error = Some(format!("Could not reach the amp worker: {error}"));
            return false;
        }
        self.waiting = Some((Instant::now(), "save"));
        self.status = format!(
            "Saving {:?} to {} {}…",
            name,
            protocol::bank_label(dialog.bank),
            dialog.index
        );
        true
    }

    /// Export one slot as a `.fuse` file. False means do not overwrite it.
    fn backup_slot(&mut self, bank: u8, index: u8) -> bool {
        let Some(preset) = self.dump.get(bank, index).cloned() else {
            return true;
        };
        let default = format!("{}-{index}.fuse", protocol::bank_label(bank));
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Fender FUSE preset", &["fuse"])
            .set_file_name(default)
            .save_file()
        else {
            self.status = "Save cancelled: no backup file chosen".into();
            return false;
        };
        match crate::fuse::export(&preset)
            .and_then(|xml| std::fs::write(&path, xml).map_err(anyhow::Error::from))
        {
            Ok(()) => {
                self.status = format!(
                    "Backed up {} {index} to {}",
                    protocol::bank_label(bank),
                    path.display()
                );
                true
            }
            Err(error) => {
                self.error = Some(format!("Backup failed: {error:#}"));
                false
            }
        }
    }
}

/// The bank and slot the dialog opens on: the current selection when it is a
/// stored preset, otherwise the first empty amp slot.
pub(crate) fn store_defaults(dump: &Dump, selected: (u8, u8)) -> (u8, u8) {
    match selected.0 {
        BANK_AMP | BANK_FX if selected.1 < slot_count(selected.0) => selected,
        _ => (BANK_AMP, first_empty(dump, BANK_AMP).unwrap_or(1)),
    }
}

pub(crate) fn slot_count(bank: u8) -> u8 {
    match bank {
        BANK_AMP => AMP_SLOTS,
        BANK_FX => FX_SLOTS,
        _ => 0,
    }
}

fn clamp_bank(bank: u8) -> u8 {
    match bank {
        BANK_AMP | BANK_FX => bank,
        _ => BANK_AMP,
    }
}

fn bank_name(bank: u8) -> &'static str {
    match bank {
        BANK_FX => "FX preset",
        _ => "Amp preset",
    }
}

/// A slot is empty until the amp reports a name for it.
pub(crate) fn slot_is_empty(dump: &Dump, bank: u8, index: u8) -> bool {
    match dump.get(bank, index) {
        Some(preset) => preset.name.is_empty(),
        None => true,
    }
}

/// The first slot without a name. Bank 1 slot 0 is the voice slot, so it is
/// not offered as a default target.
pub(crate) fn first_empty(dump: &Dump, bank: u8) -> Option<u8> {
    let first = if bank == BANK_AMP { 1 } else { 0 };
    (first..slot_count(bank)).find(|index| slot_is_empty(dump, bank, *index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump_with_names(names: &[(u8, u8, &str)]) -> Dump {
        let mut dump = Dump::new();
        for (bank, index, name) in names {
            dump.add_raw(&protocol::name_change(*bank, *index, name));
        }
        dump
    }

    #[test]
    fn the_first_empty_amp_slot_skips_the_voice_slot() {
        let dump = dump_with_names(&[(BANK_AMP, 0, "voice"), (BANK_AMP, 1, "one")]);
        assert_eq!(first_empty(&dump, BANK_AMP), Some(2));
    }

    #[test]
    fn the_default_target_is_the_current_selection_when_stored() {
        let dump = dump_with_names(&[(BANK_AMP, 4, "four"), (BANK_FX, 2, "fx")]);
        assert_eq!(store_defaults(&dump, (BANK_FX, 2)), (BANK_FX, 2));
        assert_eq!(
            store_defaults(&dump, (protocol::BANK_LIVE, 0)),
            (BANK_AMP, 1)
        );
    }

    #[test]
    fn a_full_bank_still_offers_a_slot() {
        let names: Vec<(u8, u8, &str)> = (0..AMP_SLOTS).map(|i| (BANK_AMP, i, "taken")).collect();
        let dump = dump_with_names(&names);
        assert_eq!(first_empty(&dump, BANK_AMP), None);
        let (bank, index) = store_defaults(&dump, (protocol::BANK_LIVE, 0));
        assert_eq!((bank, slot_count(bank) > index), (BANK_AMP, true));
    }
}
