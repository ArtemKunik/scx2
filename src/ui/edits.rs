//! Applying edits locally and sending live ones to the amp.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::protocol::{self, Kind, BANK_LIVE};
use crate::{fuse, models, params};

use super::worker::Cmd;
use super::App;

/// How many undo snapshots to keep.
const HISTORY_LIMIT: usize = 40;
/// Edits closer together than this are one undo step (a knob drag).
const EDIT_COALESCE: Duration = Duration::from_millis(700);

impl App {
    pub(crate) fn pending(&self) -> Vec<protocol::Change> {
        protocol::diff_dumps(&self.original, &self.dump)
    }

    /// True when the dump differs from the last opened/saved/amp state.
    pub(crate) fn dirty(&self) -> bool {
        !self.pending().is_empty()
    }

    /// Snapshot the dump before an edit, coalescing a continuous drag.
    pub(crate) fn begin_edit(&mut self) {
        let now = Instant::now();
        if self
            .last_edit
            .is_some_and(|last| now.duration_since(last) < EDIT_COALESCE)
        {
            self.last_edit = Some(now);
            return;
        }
        self.history.push(self.dump.clone());
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.future.clear();
        self.last_edit = Some(now);
    }

    pub(crate) fn undo(&mut self) {
        let Some(previous) = self.history.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.dump, previous);
        self.future.push(current);
        self.last_edit = None;
        self.status = "Undo".into();
    }

    pub(crate) fn redo(&mut self) {
        let Some(next) = self.future.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.dump, next);
        self.history.push(current);
        self.last_edit = None;
        self.status = "Redo".into();
    }

    /// Save the whole dump as JSON, or the selected preset as `.fuse`.
    pub(crate) fn save_as(&mut self, path: &Path) {
        let is_fuse = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("fuse"));
        let result = if is_fuse {
            let Some(preset) = self.dump.get(self.selected.0, self.selected.1) else {
                self.error = Some("Choose a preset before exporting a .fuse file".into());
                return;
            };
            fuse::export(preset)
                .and_then(|xml| std::fs::write(path, xml).map_err(anyhow::Error::from))
        } else {
            serde_json::to_string_pretty(&self.dump.to_json())
                .map_err(anyhow::Error::from)
                .and_then(|text| std::fs::write(path, text).map_err(anyhow::Error::from))
        };
        match result {
            Ok(()) => {
                self.path = Some(path.to_path_buf());
                if !is_fuse {
                    self.original = self.dump.clone();
                }
                self.status = format!("Saved {}", path.display());
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    /// Native Open dialog for a backup or a FUSE preset.
    pub(crate) fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("SCX2 backup", &["json"])
            .add_filter("Fender FUSE preset", &["fuse"])
            .pick_file()
        {
            self.load_file(&path);
        }
    }

    /// Native Save As dialog. The extension decides JSON or `.fuse`.
    pub(crate) fn save_dialog(&mut self) {
        let default = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("scx2-backup.json")
            .to_string();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("SCX2 backup", &["json"])
            .add_filter("Fender FUSE preset", &["fuse"])
            .set_file_name(default)
            .save_file()
        {
            self.save_as(&path);
        }
    }

    /// Push the whole live buffer to a connected amp.
    ///
    /// Used after importing a `.fuse` file, where the whole preset arrives at
    /// once instead of as a series of edits.
    pub(crate) fn send_live_preset(&mut self) {
        let Some(tx) = self.tx.as_ref().filter(|_| self.connected) else {
            return;
        };
        let Some(live) = self.dump.live() else {
            return;
        };
        for message in live.messages() {
            let _ = tx.send(Cmd::Block {
                code: message.kind().code(),
                frame: protocol::as_host_frame(message.as_bytes(), protocol::HOST_CONTROL),
            });
        }
    }

    /// Send every edit of the live buffer the moment it happens.
    pub(crate) fn send_live_edits(&mut self) {
        let Some(tx) = self.tx.as_ref().filter(|_| self.connected) else {
            return;
        };
        let changes: Vec<_> = self
            .pending()
            .into_iter()
            .filter(|change| change.bank == BANK_LIVE)
            .collect();
        let Some(live) = self.dump.get(BANK_LIVE, 0).filter(|_| !changes.is_empty()) else {
            return;
        };

        let model = live
            .block(Kind::Amp)
            .and_then(|block| block.model())
            .unwrap_or(0);
        let mut blocks = BTreeSet::new();
        for change in &changes {
            // Parameters the amp drives through direct control ignore block
            // writes; send them the way the front-panel knob does.
            let param = (change.kind == Kind::Amp)
                .then(|| params::index_for_offset(change.offset))
                .flatten();
            match param {
                Some(index) => {
                    let _ = tx.send(Cmd::SetParam {
                        model,
                        index,
                        value: change.after,
                    });
                }
                None => {
                    blocks.insert(change.kind.code());
                }
            }
        }
        for code in blocks {
            if let Some(message) = live.block(Kind::from_code(code)) {
                let _ = tx.send(Cmd::Block {
                    code,
                    frame: protocol::as_host_frame(message.as_bytes(), protocol::HOST_CONTROL),
                });
            }
        }

        let sent = live.clone();
        if let Some(known) = self.original.get_mut(BANK_LIVE, 0) {
            *known = sent;
        }
    }

    pub(crate) fn save_backup(&mut self) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = PathBuf::from(format!("scx2-backup-{stamp}.json"));
        self.save_as(&path);
    }

    /// The effect block as the amp last reported it, used to mark edits.
    pub(crate) fn effect_baseline(
        &self,
        preset: &protocol::Preset,
        kind: Kind,
    ) -> [u8; protocol::REPORT_SIZE] {
        self.original
            .get(preset.bank, preset.index)
            .and_then(|known| known.block(kind))
            .or_else(|| preset.block(kind))
            .map(|block| *block.as_bytes())
            .unwrap_or([0; protocol::REPORT_SIZE])
    }

    /// Load an effect model into the selected preset, or clear the slot with 0.
    ///
    /// The model's own parameter defaults replace whatever the previous effect
    /// had, so switching from a chorus to a phaser does not inherit the
    /// chorus's rate and depth.
    pub(crate) fn set_effect_model(&mut self, kind: Kind, model: u16) {
        self.begin_edit();
        let (bank, index) = self.selected;
        let Some(preset) = self.dump.get_mut(bank, index) else {
            return;
        };
        if !preset.load_model(kind, model) {
            return;
        }
        self.status = if model == 0 {
            format!("Cleared the {} slot", kind.label())
        } else {
            format!(
                "Loaded {} into the {} slot",
                models::model_name(kind, model).unwrap_or("effect"),
                kind.label()
            )
        };
    }

    /// Put the selected effect back to its model defaults.
    pub(crate) fn reset_effect(&mut self, kind: Kind) {
        let (bank, index) = self.selected;
        let Some(model) = self
            .dump
            .get(bank, index)
            .and_then(|preset| preset.block(kind))
            .and_then(|block| block.model())
        else {
            return;
        };
        if model == 0 {
            return;
        }
        self.begin_edit();
        if let Some(preset) = self.dump.get_mut(bank, index) {
            preset.load_model(kind, model);
        }
        self.status = format!("Reset {} parameters to defaults", kind.label());
    }

    /// Copy the effect parameter bytes that moved into the selected preset.
    pub(crate) fn apply_effect(&mut self, kind: Kind, before: &[u8], after: &[u8]) {
        for (index, (before, after)) in before.iter().zip(after).enumerate() {
            if before != after {
                self.apply_edit(kind, protocol::OFF_PARAMS + index, *after);
            }
        }
    }

    pub(crate) fn apply_edit(&mut self, kind: Kind, offset: usize, value: u8) {
        let (bank, index) = self.selected;
        let unchanged = self
            .dump
            .get(bank, index)
            .and_then(|preset| preset.block(kind))
            .is_some_and(|block| block.byte(offset) == value);
        if unchanged {
            return;
        }
        self.begin_edit();
        let Some(block) = self
            .dump
            .get_mut(bank, index)
            .and_then(|preset| preset.block_mut(kind))
        else {
            return;
        };
        block.set_byte(offset, value);

        // Voice is stored three times, and the amp only changes voice when the
        // model id moves with it (verified on hardware).
        if kind == Kind::Amp && (44..=46).contains(&offset) {
            for other in [44usize, 45, 46] {
                block.set_byte(other, value);
            }
            let model = models::voice_model_id(value);
            if model != 0 {
                let [low, high] = model.to_le_bytes();
                block.set_byte(protocol::OFF_MODEL, low);
                block.set_byte(protocol::OFF_MODEL + 1, high);
            }
        }
    }
}
