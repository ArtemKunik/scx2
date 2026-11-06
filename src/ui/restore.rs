//! "Restore Backup…": write stored presets from a JSON backup to the amp.
//!
//! The amp accepts a backup block by block, so the worker writes one report per
//! turn and reports progress; cancel stops between blocks. A full re-dump then
//! compares every restored slot with the backup. The live buffer in the backup
//! is never written: on this amp that can overwrite the selected preset.

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Align, Color32, Layout, RichText};

use super::theme::*;
use super::worker::{Cmd, RestoreReport};
use super::App;
use crate::protocol::{self, Dump, Preset, BANK_AMP, BANK_FX};

/// How long a restore may go without a progress event before it is treated as
/// stuck. A block takes about 60 ms, so this is generous.
const RESTORE_STALL: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreScope {
    All,
    Amp,
    Fx,
    One,
}

#[derive(Clone)]
pub(crate) struct RestoreDialog {
    path: PathBuf,
    device: String,
    source: Dump,
    scope: RestoreScope,
    one: (u8, u8),
    running: bool,
    done: usize,
    total: usize,
    /// When the worker last reported a block; a silent amp is a stuck restore.
    last_progress: Instant,
    cancel_sent: bool,
}

impl App {
    /// Pick a backup and open the restore dialog for it.
    pub(super) fn open_restore_dialog(&mut self) {
        if self.amp_busy() || self.store.is_some() || self.rename.is_some() || self.import.is_some()
        {
            self.status = "Finish the open dialog or wait for the amp first.".into();
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SCX2 backup", &["json"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(format!("Could not read {}: {error}", path.display()));
                return;
            }
        };
        let source = match Dump::from_json(&text) {
            Ok(dump) => dump,
            Err(error) => {
                self.error = Some(format!("{} is not an SCX2 backup: {error}", path.display()));
                return;
            }
        };
        let device = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|value| {
                value
                    .get("device")
                    .and_then(|device| device.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "SCX2 backup".into());
        let stored = source.bank(BANK_AMP).count() + source.bank(BANK_FX).count();
        if stored == 0 {
            self.error = Some(format!(
                "{} has no stored presets to restore.",
                path.display()
            ));
            return;
        }
        let one = first_source_slot(&source);
        self.restore = Some(RestoreDialog {
            path,
            device,
            source,
            scope: RestoreScope::All,
            one,
            running: false,
            done: 0,
            total: 0,
            last_progress: Instant::now(),
            cancel_sent: false,
        });
    }

    pub(super) fn restore_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.restore.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        let mut stop = false;

        let plan = plan(&dialog.source, dialog.scope, dialog.one);
        let blocks: usize = plan.iter().map(|preset| preset.messages().len()).sum();

        egui::Window::new("Restore Backup")
            .id(egui::Id::new("restore-backup"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(460.0);
                let file = dialog
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("backup.json");
                ui.label(
                    RichText::new(format!(
                        "{} · {} amp + {} FX preset(s)",
                        dialog.device,
                        dialog.source.bank(BANK_AMP).count(),
                        dialog.source.bank(BANK_FX).count(),
                    ))
                    .font(regular(11.5))
                    .color(TEXT_3),
                );
                ui.label(
                    RichText::new(file)
                        .font(regular(11.0))
                        .color(TEXT_3),
                );
                ui.add_space(10.0);

                if dialog.running {
                    let fraction = if dialog.total == 0 {
                        0.0
                    } else {
                        dialog.done as f32 / dialog.total as f32
                    };
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .text(format!("block {} of {}", dialog.done, dialog.total))
                            .show_percentage(),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Keep the amp on. Cancel stops after the block that is being written.",
                        )
                        .font(regular(11.0))
                        .color(TEXT_2),
                    );
                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if dialog.cancel_sent {
                            ui.add_enabled(
                                false,
                                egui::Button::new(
                                    RichText::new("Stopping…").font(regular(12.5)),
                                ),
                            );
                        } else if ui
                            .add(egui::Button::new(
                                RichText::new("Cancel restore").font(regular(12.5)),
                            ))
                            .clicked()
                        {
                            stop = true;
                        }
                    });
                } else {
                    ui.label(RichText::new("Scope").font(semibold(12.0)).color(TEXT));
                    ui.add_space(2.0);
                    ui.radio_value(
                        &mut dialog.scope,
                        RestoreScope::All,
                        format!(
                            "All stored presets — {} amp, {} FX",
                            dialog.source.bank(BANK_AMP).count(),
                            dialog.source.bank(BANK_FX).count()
                        ),
                    );
                    ui.radio_value(
                        &mut dialog.scope,
                        RestoreScope::Amp,
                        format!(
                            "Amp presets only — {} slot(s)",
                            dialog.source.bank(BANK_AMP).count()
                        ),
                    );
                    ui.radio_value(
                        &mut dialog.scope,
                        RestoreScope::Fx,
                        format!(
                            "FX presets only — {} slot(s)",
                            dialog.source.bank(BANK_FX).count()
                        ),
                    );
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dialog.scope, RestoreScope::One, "One preset");
                        egui::ComboBox::from_id_source("restore-one")
                            .selected_text(source_slot_label(&dialog.source, dialog.one))
                            .show_ui(ui, |ui| {
                                for preset in stored_presets(&dialog.source) {
                                    let key = (preset.bank, preset.index);
                                    ui.selectable_value(
                                        &mut dialog.one,
                                        key,
                                        format!(
                                            "{} {} · {}",
                                            protocol::bank_label(preset.bank),
                                            preset.index,
                                            if preset.name.is_empty() {
                                                "unnamed"
                                            } else {
                                                preset.name.as_str()
                                            }
                                        ),
                                    );
                                }
                            });
                    });
                    ui.add_space(8.0);

                    ui.label(RichText::new("Will write").font(semibold(12.0)).color(TEXT));
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical()
                        .max_height(150.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for preset in &plan {
                                let current = self
                                    .dump
                                    .get(preset.bank, preset.index)
                                    .map(|known| known.name.clone())
                                    .unwrap_or_default();
                                ui.label(
                                    RichText::new(format!(
                                        "{} {} · {:?} → replaces {:?}",
                                        protocol::bank_label(preset.bank),
                                        preset.index,
                                        display_name(&preset.name),
                                        display_name(&current),
                                    ))
                                    .font(regular(11.0))
                                    .color(TEXT_2),
                                );
                            }
                        });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "{} block(s) in {} slot(s). The backup's live buffer is not written.",
                            blocks,
                            plan.len()
                        ))
                        .font(regular(11.0))
                        .color(TEXT_3),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "A backup of the current amp state is saved before anything is written.",
                        )
                        .font(regular(11.0))
                        .color(TEXT_3),
                    );

                    if !self.connected {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "Not connected — this only reports what would be written.",
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

                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let ready =
                            !plan.is_empty() && (self.synced || !self.connected);
                        let label = if self.connected {
                            "Back up and restore"
                        } else {
                            "Dry run"
                        };
                        let go = egui::Button::new(
                            RichText::new(label)
                                .font(semibold(13.0))
                                .color(if ready { AMBER_INK } else { TEXT_3 }),
                        )
                        .fill(if ready {
                            AMBER
                        } else {
                            Color32::from_white_alpha(8)
                        });
                        if ui.add_enabled(ready, go).clicked() {
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
                }
            });

        if stop {
            dialog.cancel_sent = true;
            if let Some(tx) = &self.tx {
                let _ = tx.send(Cmd::CancelRestore);
            }
            self.status = "Stopping after the current block…".into();
            self.restore = Some(dialog);
            return;
        }
        if confirm {
            if !self.commit_restore(dialog.clone()) {
                self.restore = Some(dialog);
            }
            return;
        }
        // A running restore keeps its window until the worker reports back,
        // even if the close button was used.
        if (cancel || !open) && !dialog.running {
            self.restore = None;
            return;
        }
        if !open && dialog.running {
            dialog.cancel_sent = true;
            if let Some(tx) = &self.tx {
                let _ = tx.send(Cmd::CancelRestore);
            }
        }
        self.restore = Some(dialog);
    }

    /// Ask for the mandatory pre-restore backup, then hand the plan to the
    /// worker. Returns false when nothing was sent and the dialog should stay.
    fn commit_restore(&mut self, mut dialog: RestoreDialog) -> bool {
        let presets = plan(&dialog.source, dialog.scope, dialog.one);
        let blocks: usize = presets.iter().map(|preset| preset.messages().len()).sum();
        if presets.is_empty() {
            self.error = Some("That scope has nothing to restore.".into());
            return false;
        }
        if !self.connected {
            self.status = format!(
                "Dry run: would write {blocks} block(s) into {} slot(s) — connect the amp to restore",
                presets.len()
            );
            return true;
        }
        if !self.backup_before_restore() {
            return false;
        }
        let Some(tx) = self.tx.as_ref() else {
            self.error = Some("The amp worker is not running.".into());
            return false;
        };
        if let Err(error) = tx.send(Cmd::Restore { presets }) {
            self.error = Some(format!("Could not reach the amp worker: {error}"));
            return false;
        }
        dialog.running = true;
        dialog.done = 0;
        dialog.total = blocks;
        dialog.last_progress = Instant::now();
        dialog.cancel_sent = false;
        self.restore = Some(dialog);
        self.status = format!("Restoring {blocks} block(s)…");
        true
    }

    /// Write the current dump out before a restore. False stops the restore.
    fn backup_before_restore(&mut self) -> bool {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("SCX2 backup", &["json"])
            .set_file_name(format!("scx2-before-restore-{stamp}.json"))
            .save_file()
        else {
            self.status = "Restore cancelled: no backup file chosen".into();
            return false;
        };
        let result = serde_json::to_string_pretty(&self.dump.to_json())
            .map_err(anyhow::Error::from)
            .and_then(|text| std::fs::write(&path, text).map_err(anyhow::Error::from));
        match result {
            Ok(()) => {
                self.status = format!("Backed up the current amp state to {}", path.display());
                true
            }
            Err(error) => {
                self.error = Some(format!("Backup failed: {error:#}"));
                false
            }
        }
    }

    /// A restore event from the worker.
    pub(super) fn on_restore_progress(&mut self, done: usize, total: usize) {
        if let Some(dialog) = &mut self.restore {
            dialog.done = done;
            dialog.total = total;
            dialog.last_progress = Instant::now();
        }
    }

    /// The restore stopped and the re-dump has been compared with the backup.
    pub(super) fn on_restore_done(&mut self, report: RestoreReport) {
        self.restore = None;
        if report.cancelled {
            let headline = format!(
                "Restore cancelled after {} of {} blocks; re-check the slots below.",
                report.written, report.total
            );
            if report.mismatches.is_empty() {
                self.status = headline;
            } else {
                self.error = Some(format!(
                    "{headline}\nThe re-dump does not match the backup for:\n{}",
                    report
                        .mismatches
                        .iter()
                        .map(|line| format!("  {line}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
            return;
        }
        if report.mismatches.is_empty() {
            self.status = format!(
                "Restored {} blocks into {} slot(s) · re-dump matches the backup",
                report.written, report.slots
            );
        } else {
            self.error = Some(format!(
                "Restored {} blocks, but {} slot(s) do not match the backup:\n{}",
                report.written,
                report.mismatches.len(),
                report
                    .mismatches
                    .iter()
                    .map(|line| format!("  {line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }

    /// True while another amp operation owns the device.
    pub(crate) fn amp_busy(&self) -> bool {
        self.waiting.is_some() || self.restore.as_ref().is_some_and(|dialog| dialog.running)
    }

    /// True when a restore has gone quiet for too long.
    pub(super) fn restore_stalled(&self) -> bool {
        self.restore
            .as_ref()
            .is_some_and(|dialog| dialog.running && dialog.last_progress.elapsed() > RESTORE_STALL)
    }

    /// The restore dialog is modal in behaviour: a second device operation
    /// must not start while one is open.
    pub(super) fn restore_dialog_open(&self) -> bool {
        self.restore.is_some()
    }
}

impl RestoreDialog {
    pub(super) fn is_running(&self) -> bool {
        self.running
    }
}

/// The presets a scope selects, in bank/index order.
pub(crate) fn plan(source: &Dump, scope: RestoreScope, one: (u8, u8)) -> Vec<Preset> {
    match scope {
        RestoreScope::All => source
            .bank(BANK_AMP)
            .chain(source.bank(BANK_FX))
            .cloned()
            .collect(),
        RestoreScope::Amp => source.bank(BANK_AMP).cloned().collect(),
        RestoreScope::Fx => source.bank(BANK_FX).cloned().collect(),
        RestoreScope::One => source.get(one.0, one.1).cloned().into_iter().collect(),
    }
}

fn stored_presets(source: &Dump) -> impl Iterator<Item = &Preset> {
    source.bank(BANK_AMP).chain(source.bank(BANK_FX))
}

/// The first stored slot a single-slot scope should point at: the first named
/// slot, otherwise the first slot the backup has.
fn first_source_slot(source: &Dump) -> (u8, u8) {
    stored_presets(source)
        .find(|preset| !preset.name.is_empty())
        .or_else(|| stored_presets(source).next())
        .map(|preset| (preset.bank, preset.index))
        .unwrap_or((BANK_AMP, 0))
}

fn source_slot_label(source: &Dump, key: (u8, u8)) -> String {
    match source.get(key.0, key.1) {
        Some(preset) => format!(
            "{} {} · {}",
            protocol::bank_label(key.0),
            key.1,
            display_name(&preset.name)
        ),
        None => format!("{} {}", protocol::bank_label(key.0), key.1),
    }
}

fn display_name(name: &str) -> &str {
    if name.is_empty() {
        "unnamed"
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Message, BANK_LIVE};

    fn dump_with(slots: &[(u8, u8, &str)]) -> Dump {
        let mut dump = Dump::new();
        for (bank, index, name) in slots {
            let mut preset = Preset::new(*bank, *index);
            preset.add(
                Message::from_bytes(protocol::name_change(*bank, *index, name))
                    .expect("a valid name block"),
            );
            dump.insert(preset);
        }
        dump
    }

    #[test]
    fn the_all_scope_restores_both_banks_and_never_the_live_buffer() {
        let dump = dump_with(&[
            (BANK_LIVE, 0, "live"),
            (BANK_AMP, 1, "one"),
            (BANK_FX, 2, "two"),
        ]);
        let keys: Vec<(u8, u8)> = plan(&dump, RestoreScope::All, (BANK_AMP, 1))
            .iter()
            .map(|preset| (preset.bank, preset.index))
            .collect();
        assert_eq!(keys, vec![(BANK_AMP, 1), (BANK_FX, 2)]);
    }

    #[test]
    fn a_bank_scope_returns_only_that_bank() {
        let dump = dump_with(&[
            (BANK_LIVE, 0, "live"),
            (BANK_AMP, 1, "one"),
            (BANK_AMP, 2, "two"),
            (BANK_FX, 3, "three"),
        ]);
        assert_eq!(plan(&dump, RestoreScope::Amp, (BANK_AMP, 1)).len(), 2);
        let fx = plan(&dump, RestoreScope::Fx, (BANK_AMP, 1));
        assert_eq!(fx.len(), 1);
        assert_eq!(fx[0].bank, BANK_FX);
    }

    #[test]
    fn the_single_scope_returns_exactly_the_chosen_slot() {
        let dump = dump_with(&[(BANK_AMP, 1, "one"), (BANK_FX, 3, "three")]);
        let chosen = plan(&dump, RestoreScope::One, (BANK_FX, 3));
        assert_eq!(chosen.len(), 1);
        assert_eq!((chosen[0].bank, chosen[0].index), (BANK_FX, 3));
        assert!(plan(&dump, RestoreScope::One, (BANK_FX, 9)).is_empty());
    }

    #[test]
    fn the_single_scope_defaults_to_the_first_named_amp_preset() {
        let dump = dump_with(&[(BANK_AMP, 0, ""), (BANK_AMP, 1, "one")]);
        assert_eq!(first_source_slot(&dump), (BANK_AMP, 1));
    }

    fn dialog_for(source: Dump) -> RestoreDialog {
        RestoreDialog {
            path: PathBuf::from("backup.json"),
            device: "Fender Super Champ X2".into(),
            source,
            scope: RestoreScope::All,
            one: (BANK_AMP, 1),
            running: false,
            done: 0,
            total: 0,
            last_progress: Instant::now(),
            cancel_sent: false,
        }
    }

    fn test_app(ctx: &egui::Context) -> App {
        super::super::theme::install(ctx);
        let mut app = App::fresh(ctx.clone());
        app.dump = dump_with(&[(BANK_AMP, 1, "old one")]);
        app.original = app.dump.clone();
        app.synced = true;
        app.connected = false;
        app
    }

    #[test]
    fn the_restore_dialog_renders_with_its_scope_and_targets() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        app.restore = Some(dialog_for(dump_with(&[
            (BANK_AMP, 1, "one"),
            (BANK_FX, 2, "two"),
        ])));
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.restore_window(ctx));
        }
        assert!(app.restore.is_some(), "the dialog stays open while idle");
    }

    #[test]
    fn a_restore_dry_run_reports_the_plan_without_writing() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        let dialog = dialog_for(dump_with(&[(BANK_AMP, 1, "one"), (BANK_FX, 2, "two")]));
        assert!(app.commit_restore(dialog));
        assert!(app.status.starts_with("Dry run: "), "{}", app.status);
        assert!(app.status.contains("2 block(s)"), "{}", app.status);
        assert!(app.status.contains("2 slot(s)"), "{}", app.status);
    }

    #[test]
    fn a_running_restore_keeps_other_device_dialogs_shut() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        let mut dialog = dialog_for(dump_with(&[(BANK_AMP, 1, "one")]));
        dialog.running = true;
        app.restore = Some(dialog);
        assert!(app.amp_busy());
        app.open_store_dialog();
        assert!(app.store.is_none());
        app.open_rename_dialog(BANK_AMP, 1);
        assert!(app.rename.is_none());
    }

    #[test]
    fn the_progress_view_renders() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        let mut dialog = dialog_for(dump_with(&[(BANK_AMP, 1, "one")]));
        dialog.running = true;
        dialog.done = 3;
        dialog.total = 7;
        app.restore = Some(dialog);
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.restore_window(ctx));
        }
        assert!(app.restore.is_some());
    }

    #[test]
    fn a_finished_restore_reports_the_match() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        app.restore = Some(dialog_for(dump_with(&[(BANK_AMP, 1, "one")])));
        app.on_restore_done(RestoreReport {
            written: 2,
            total: 2,
            slots: 1,
            cancelled: false,
            mismatches: Vec::new(),
        });
        assert!(app.restore.is_none());
        assert!(
            app.status.contains("re-dump matches the backup"),
            "{}",
            app.status
        );
    }

    #[test]
    fn a_mismatch_after_a_restore_lists_every_slot() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        app.restore = Some(dialog_for(dump_with(&[(BANK_AMP, 1, "one")])));
        app.on_restore_done(RestoreReport {
            written: 2,
            total: 2,
            slots: 1,
            cancelled: false,
            mismatches: vec!["amp 1: 2 byte(s) differ".into()],
        });
        let error = app.error.expect("a mismatch is an error");
        assert!(error.contains("amp 1: 2 byte(s) differ"), "{error}");
    }

    #[test]
    fn a_cancelled_restore_says_where_it_stopped() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        app.restore = Some(dialog_for(dump_with(&[(BANK_AMP, 1, "one")])));
        app.on_restore_done(RestoreReport {
            written: 3,
            total: 9,
            slots: 1,
            cancelled: true,
            mismatches: vec!["amp 1: partially written".into()],
        });
        let error = app.error.expect("a partial slot is worth an error");
        assert!(error.contains("cancelled after 3 of 9"), "{error}");
        assert!(error.contains("partially written"), "{error}");
    }
}
