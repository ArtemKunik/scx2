//! FUSE's save dialogs: **Save to Amp…**, **Save Effects**, **Save Voice** and
//! **Save CH1**, all on the verified save-preset frame.
//!
//! The amp copies its live buffer into a slot; there is no per-block payload to
//! send. The dialog therefore shows what is about to be replaced, offers a
//! backup of the target slot, and the worker reads the slot back afterwards.
//! The sub-preset dialogs differ only in the bank they target and in what the
//! amp stores there.

use std::time::Instant;

use eframe::egui::{self, Align, Color32, Layout, RichText};

use super::name::{fit_name, fits, name_input};
use super::theme::*;
use super::worker::Cmd;
use super::App;
use crate::models;
use crate::protocol::{self, Dump, Kind, BANK_AMP, BANK_FX, BANK_LIVE};

/// The amp's 16 voices occupy bank 1 slots 0..=15.
const VOICE_SLOTS: u8 = 16;
/// Bank 1 slot 16 holds channel 1, the clean tube channel (`0x00eb`).
const CLEAN_SLOT: u8 = 16;
/// Bank 1 has 17 slots: 0..=15 are the voices, 16 is the clean channel.
const AMP_SLOTS: u8 = 17;
/// Bank 2 has 16 effect presets.
const FX_SLOTS: u8 = 16;

/// Which of FUSE's save dialogs this is.
///
/// All three send the same save-preset frame; they differ in the bank they
/// target and in what the amp stores there. `Preset` is the general dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoreMode {
    Preset,
    /// SAVE EFFECTS: bank 2 keeps modulation, delay and reverb.
    Effects,
    /// SAVE VOICE: bank 1 slots 0..=15 keep the amp and stomp.
    Voice,
    /// SAVE CH1: bank 1 slot 16 keeps channel 1.
    Clean,
}

impl StoreMode {
    fn title(self) -> &'static str {
        match self {
            StoreMode::Preset => "Save to Amp",
            StoreMode::Effects => "Save Effects",
            StoreMode::Voice => "Save Voice",
            StoreMode::Clean => "Save CH1",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            StoreMode::Preset => "Writes the live buffer into a stored slot.",
            StoreMode::Effects => {
                "Writes the modulation, delay and reverb blocks into an FX preset."
            }
            StoreMode::Voice => {
                "Writes the amp, cabinet and stomp blocks into one of the 16 voices."
            }
            StoreMode::Clean => {
                "Writes channel 1 (the clean tube channel) into the amp's Clean Channel slot."
            }
        }
    }

    fn action(self) -> &'static str {
        match self {
            StoreMode::Preset => "Save to Amp",
            StoreMode::Effects => "Save Effects",
            StoreMode::Voice => "Save Voice",
            StoreMode::Clean => "Save CH1",
        }
    }

    /// The bank this mode writes, or `None` when the person picks it.
    fn bank(self) -> Option<u8> {
        match self {
            StoreMode::Preset => None,
            StoreMode::Effects => Some(BANK_FX),
            StoreMode::Voice | StoreMode::Clean => Some(BANK_AMP),
        }
    }

    fn slots(self, bank: u8) -> (u8, u8) {
        match self {
            StoreMode::Preset => (0, slot_count(bank).saturating_sub(1)),
            StoreMode::Effects => (0, FX_SLOTS - 1),
            StoreMode::Voice => (0, VOICE_SLOTS - 1),
            StoreMode::Clean => (CLEAN_SLOT, CLEAN_SLOT),
        }
    }
}

#[derive(Clone)]
pub(crate) struct StoreDialog {
    mode: StoreMode,
    bank: u8,
    index: u8,
    name: String,
    /// Write the slot's current contents out before replacing them.
    backup: bool,
    /// Once the person types a name, slot changes stop replacing it.
    name_touched: bool,
}

impl App {
    /// The toolbar's general dialog: bank and slot are free.
    pub(super) fn open_store_dialog(&mut self) {
        self.open_store(StoreMode::Preset);
    }

    /// SAVE EFFECTS / SAVE VOICE / SAVE CH1 from the sidebar.
    pub(super) fn open_sub_preset(&mut self, mode: StoreMode) {
        self.open_store(mode);
    }

    fn open_store(&mut self, mode: StoreMode) {
        if self.amp_busy() || self.restore_dialog_open() || self.import.is_some() {
            self.status = "Another amp operation is still running; try again in a moment.".into();
            return;
        }
        let (bank, index) = store_target(mode, &self.dump, self.selected);
        let name = self
            .dump
            .get(bank, index)
            .map(|preset| preset.name.clone())
            .unwrap_or_default();
        self.store = Some(StoreDialog {
            mode,
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

        egui::Window::new(dialog.mode.title())
            .id(egui::Id::new("save-to-amp"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(
                    RichText::new(dialog.mode.blurb())
                        .font(regular(11.5))
                        .color(TEXT_3),
                );
                ui.add_space(10.0);

                // Name -----------------------------------------------------------------
                if name_input(ui, &mut dialog.name) {
                    dialog.name_touched = true;
                }
                ui.add_space(10.0);

                // Slot -----------------------------------------------------------------
                ui.label(RichText::new("Slot").font(semibold(12.0)).color(TEXT));
                ui.add_space(2.0);
                if dialog.mode == StoreMode::Clean {
                    ui.label(
                        RichText::new(format!(
                            "Amp preset {CLEAN_SLOT} · {}",
                            slot_name(&self.dump, BANK_AMP, CLEAN_SLOT)
                        ))
                        .font(regular(12.5))
                        .color(TEXT),
                    );
                    if !live_is_clean(&self.dump) {
                        let warning = match live_amp_model(&self.dump) {
                            Some(_) => format!(
                                "The amp is on channel 2 ({}). Switch it to channel 1 before saving CH1.",
                                live_voice_name(&self.dump)
                            ),
                            None => {
                                "The live buffer has no amp block to save CH1 from.".to_string()
                            }
                        };
                        ui.label(
                            RichText::new(warning)
                                .font(regular(11.0))
                                .color(ERROR),
                        );
                    }
                } else {
                    if let Some(bank) = dialog.mode.bank() {
                        ui.label(
                            RichText::new(format!("Bank: {}", bank_title(bank)))
                                .font(regular(11.5))
                                .color(TEXT_2),
                        );
                    } else {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut dialog.bank, BANK_AMP, "Amp presets");
                            ui.selectable_value(&mut dialog.bank, BANK_FX, "FX presets");
                        });
                        dialog.bank = clamp_bank(dialog.bank);
                    }
                    let (first, last) = dialog.mode.slots(dialog.bank);
                    if !(first..=last).contains(&dialog.index) {
                        dialog.index = first;
                    }
                    ui.add_space(6.0);
                    egui::Grid::new("store-slots")
                        .num_columns(6)
                        .spacing([4.0, 4.0])
                        .show(ui, |ui| {
                            for index in first..=last {
                                let slot = slot_name(&self.dump, dialog.bank, index);
                                let label = if dialog.bank == BANK_AMP && index == 0 {
                                    format!("{index} · voice 1 · {slot}")
                                } else {
                                    format!("{index} · {slot}")
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
                                if (index + 1 - first) % 6 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                }
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
                        RichText::new("Slot 0 is voice 1; storing into it is untested.")
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
                    let clean_ok = dialog.mode != StoreMode::Clean || live_is_clean(&self.dump);
                    let ready =
                        fits(&dialog.name) && (self.synced || !self.connected) && clean_ok;
                    let save = egui::Button::new(
                        RichText::new(dialog.mode.action())
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
        if dialog.mode == StoreMode::Clean && !live_is_clean(&self.dump) {
            self.error = Some(
                "The amp is not on channel 1. Switch it to channel 1 and save CH1 again.".into(),
            );
            return false;
        }
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

/// Where a save dialog points when it opens.
pub(crate) fn store_target(mode: StoreMode, dump: &Dump, selected: (u8, u8)) -> (u8, u8) {
    match mode {
        StoreMode::Preset => store_defaults(dump, selected),
        StoreMode::Effects => {
            let index = if selected.0 == BANK_FX && selected.1 < FX_SLOTS {
                selected.1
            } else {
                first_empty_between(dump, BANK_FX, 0, FX_SLOTS - 1).unwrap_or(0)
            };
            (BANK_FX, index)
        }
        StoreMode::Voice => {
            let index = if selected.0 == BANK_AMP && selected.1 < VOICE_SLOTS {
                selected.1
            } else {
                first_empty_between(dump, BANK_AMP, 0, VOICE_SLOTS - 1).unwrap_or(0)
            };
            (BANK_AMP, index)
        }
        StoreMode::Clean => (BANK_AMP, CLEAN_SLOT),
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

fn bank_title(bank: u8) -> &'static str {
    match bank {
        BANK_FX => "FX presets",
        _ => "Amp presets",
    }
}

/// The name of a slot, or `empty` when the amp reports none.
fn slot_name(dump: &Dump, bank: u8, index: u8) -> String {
    match dump
        .get(bank, index)
        .filter(|preset| !preset.name.is_empty())
    {
        Some(preset) => preset.name.clone(),
        None => "empty".to_string(),
    }
}

/// True when the live buffer is on channel 1, the clean tube channel.
fn live_is_clean(dump: &Dump) -> bool {
    live_amp_model(dump) == Some(models::CLEAN_CHANNEL_MODEL)
}

fn live_amp_model(dump: &Dump) -> Option<u16> {
    dump.get(BANK_LIVE, 0)?.block(Kind::Amp)?.model()
}

fn live_voice_name(dump: &Dump) -> String {
    match live_amp_model(dump) {
        Some(model) => models::model_name(Kind::Amp, model)
            .map(str::to_string)
            .unwrap_or_else(|| format!("unknown 0x{model:04x}")),
        None => "no amp block".to_string(),
    }
}

/// A slot is empty until the amp reports a name for it.
pub(crate) fn slot_is_empty(dump: &Dump, bank: u8, index: u8) -> bool {
    match dump.get(bank, index) {
        Some(preset) => preset.name.is_empty(),
        None => true,
    }
}

/// The first slot without a name. Bank 1 slot 0 is voice 1, so it is not
/// offered as a general save target.
pub(crate) fn first_empty(dump: &Dump, bank: u8) -> Option<u8> {
    let first = if bank == BANK_AMP { 1 } else { 0 };
    first_empty_between(dump, bank, first, slot_count(bank).saturating_sub(1))
}

fn first_empty_between(dump: &Dump, bank: u8, first: u8, last: u8) -> Option<u8> {
    (first..=last).find(|index| slot_is_empty(dump, bank, *index))
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

    #[test]
    fn the_sub_preset_dialogs_pick_their_bank_and_slot() {
        let dump = dump_with_names(&[(BANK_AMP, 1, "one"), (BANK_FX, 2, "two")]);
        assert_eq!(
            store_target(StoreMode::Effects, &dump, (BANK_AMP, 1)),
            (BANK_FX, 0)
        );
        assert_eq!(
            store_target(StoreMode::Voice, &dump, (BANK_AMP, 3)),
            (BANK_AMP, 3)
        );
        assert_eq!(
            store_target(StoreMode::Clean, &dump, (BANK_FX, 2)),
            (BANK_AMP, CLEAN_SLOT)
        );
    }

    #[test]
    fn save_voice_does_not_offer_the_clean_channel_slot() {
        // A full voice range: the clean slot must not be chosen by default.
        let names: Vec<(u8, u8, &str)> = (1..=16).map(|i| (BANK_AMP, i, "taken")).collect();
        let dump = dump_with_names(&names);
        let (bank, index) = store_target(StoreMode::Voice, &dump, (protocol::BANK_LIVE, 0));
        assert_eq!(bank, BANK_AMP);
        assert!(index < VOICE_SLOTS, "slot {index} is not a voice slot");
    }

    fn live_app(clean: bool) -> (egui::Context, App) {
        use crate::protocol::{Message, Preset, AMP_TO_HOST, FRAME_MARKER, OFF_MODEL, REPORT_SIZE};

        let ctx = egui::Context::default();
        super::super::theme::install(&ctx);
        let mut app = App::fresh(ctx.clone());
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = Kind::Amp.code();
        raw[6] = 1;
        let model = if clean {
            models::CLEAN_CHANNEL_MODEL
        } else {
            0x0064
        };
        let [low, high] = model.to_le_bytes();
        raw[OFF_MODEL] = low;
        raw[OFF_MODEL + 1] = high;
        let mut preset = Preset::new(BANK_LIVE, 0);
        preset.add(Message::from_bytes(raw).expect("a valid amp block"));
        app.dump.insert(preset);
        app.original = app.dump.clone();
        app.synced = true;
        (ctx, app)
    }

    #[test]
    fn channel_one_is_recognised_by_the_clean_model() {
        let (_, clean) = live_app(true);
        let (_, voice) = live_app(false);
        assert!(live_is_clean(&clean.dump));
        assert!(!live_is_clean(&voice.dump));
    }

    #[test]
    fn save_ch1_refuses_to_write_a_channel_two_voice() {
        let (_, mut app) = live_app(false);
        app.connected = false;
        app.open_sub_preset(StoreMode::Clean);
        let dialog = app.store.clone().expect("the dialog is open");
        assert!(!app.commit_store(&dialog));
        assert!(
            app.error.as_deref().unwrap_or("").contains("channel 1"),
            "{:?}",
            app.error
        );
    }

    #[test]
    fn the_sub_preset_dialogs_render() {
        for mode in [StoreMode::Effects, StoreMode::Voice, StoreMode::Clean] {
            let (ctx, mut app) = live_app(mode == StoreMode::Clean);
            app.open_sub_preset(mode);
            let dialog = app.store.clone().expect("the dialog is open");
            assert_eq!(dialog.mode, mode);
            for _ in 0..3 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.store_window(ctx));
            }
            assert!(app.store.is_some(), "{mode:?} stays open while idle");
        }
    }
}
