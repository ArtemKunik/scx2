//! Applying edits locally and sending live ones to the amp.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::protocol::{self, Kind, BANK_LIVE};
use crate::{fuse, models, params};

use super::app::PendingLive;
use super::worker::Cmd;
use super::App;

/// How many undo snapshots to keep.
const HISTORY_LIMIT: usize = 40;
/// Edits closer together than this are one undo step (a knob drag).
const EDIT_COALESCE: Duration = Duration::from_millis(700);
/// Hold a local live-buffer value this long while waiting for the amp to echo
/// it back. A full dump can arrive before the amp has picked up a write.
pub(crate) const PENDING_TTL: Duration = Duration::from_secs(5);

impl App {
    pub(crate) fn pending(&self) -> Vec<protocol::Change> {
        protocol::diff_dumps(&self.original, &self.dump)
    }

    /// True when the amp is playing channel 1, the clean tube channel.
    ///
    /// Channel 1 is not a DSP voice: the amp ignores writes to its amp and
    /// stomp blocks, and only the effects section (mod, delay, reverb) answers.
    /// The app uses this to explain why a voice or stompbox did not load
    /// instead of letting it vanish on the next dump.
    pub(crate) fn on_clean_channel(&self) -> bool {
        self.connected
            && self
                .dump
                .live()
                .and_then(|live| live.block(Kind::Amp))
                .and_then(|block| block.model())
                == Some(models::CLEAN_CHANNEL_MODEL)
    }

    fn note_clean_channel(&mut self, what: &str) {
        self.error = Some(format!(
            "The amp reports the Clean Channel. Switch to Channel 2 with the amp's Channel \
             button (and turn its Voice knob off Clean Channel), then change the {what}."
        ));
    }

    /// True when the dump differs from the last opened/saved/amp state.
    ///
    /// Library metadata counts too: it is saved in the backup, so losing it on
    /// close would be the same kind of loss as an unsent knob move.
    pub(crate) fn dirty(&self) -> bool {
        !self.pending().is_empty() || self.metadata_edited()
    }

    /// True when a preset's info differs from the last loaded/saved copy.
    fn metadata_edited(&self) -> bool {
        self.dump.presets().any(|preset| {
            self.original
                .get(preset.bank, preset.index)
                .map(|original| original.info != preset.info)
                .unwrap_or(true)
        })
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
        self.pending_live.clear();
        self.remember_live_diff();
        self.status = "Undo".into();
    }

    pub(crate) fn redo(&mut self) {
        let Some(next) = self.future.pop() else {
            return;
        };
        let current = std::mem::replace(&mut self.dump, next);
        self.history.push(current);
        self.last_edit = None;
        self.pending_live.clear();
        self.remember_live_diff();
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
        let mut bypasses = BTreeMap::new();
        for change in &changes {
            // Bypass has its own verified, instant frame and does not need an
            // execute. A block write already carries offset 22, so skip the
            // separate frame when that block is going out anyway.
            if change.kind.has_slot() && change.offset == protocol::OFF_BYPASS {
                let slot = live
                    .block(change.kind)
                    .and_then(|block| block.slot())
                    .unwrap_or(0);
                bypasses.insert(change.kind.code(), (change.kind, slot, change.after != 0));
                continue;
            }
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
        for code in &blocks {
            bypasses.remove(code);
        }

        // An effect/block write makes the amp re-stream its state. Treat the
        // current local amp settings as authoritative through that churn, so
        // old parameter echoes cannot pull the front-panel knobs back.
        if !blocks.is_empty() || !bypasses.is_empty() {
            if let Some(amp) = live.block(Kind::Amp) {
                let seen = Instant::now();
                for offset in protocol::OFF_PARAMS..protocol::REPORT_SIZE {
                    self.pending_live.insert(
                        (Kind::Amp.code(), offset),
                        PendingLive {
                            value: amp.byte(offset),
                            seen,
                        },
                    );
                }
            }
        }

        for (_, (kind, slot, bypassed)) in bypasses {
            let _ = tx.send(Cmd::Bypass {
                kind,
                slot,
                bypassed,
            });
        }
        for code in blocks {
            if let Some(message) = live.block(Kind::from_code(code)) {
                let _ = tx.send(Cmd::Block {
                    code,
                    frame: protocol::as_host_frame(message.as_bytes(), protocol::HOST_CONTROL),
                });
            }
        }

        // A write can make the amp stream older parameter echoes while it
        // catches up. Values stay protected by their own timestamps, and a
        // block/bypass write snapshots the whole amp panel before sending.
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
        if kind == Kind::Stomp && model != 0 && self.on_clean_channel() {
            self.note_clean_channel("stompbox");
            return;
        }
        self.begin_edit();
        let (bank, index) = self.selected;
        let before = self.dump.get(bank, index).cloned();
        let Some(preset) = self.dump.get_mut(bank, index) else {
            return;
        };
        if !preset.load_model(kind, model) {
            return;
        }
        self.remember_live_preset(before.as_ref());
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
        let before = self.dump.get(bank, index).cloned();
        if let Some(preset) = self.dump.get_mut(bank, index) {
            preset.load_model(kind, model);
        }
        self.remember_live_preset(before.as_ref());
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
        // Channel 1 is the analogue clean channel. The amp accepts these
        // frames but does not apply them to the DSP, so a local edit would
        // only last until the next dump. Say so instead.
        if kind == Kind::Amp && self.on_clean_channel() {
            self.note_clean_channel("amp voice or control");
            return;
        }
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
        let mut touched = Vec::new();
        let Some(block) = self
            .dump
            .get_mut(bank, index)
            .and_then(|preset| preset.block_mut(kind))
        else {
            return;
        };
        block.set_byte(offset, value);
        touched.push((offset, value));

        // Voice is stored three times, and the amp only changes voice when the
        // model id moves with it (verified on hardware).
        if kind == Kind::Amp && (44..=46).contains(&offset) {
            for other in [44usize, 45, 46] {
                block.set_byte(other, value);
                touched.push((other, value));
            }
            let model = models::voice_model_id(value);
            if model != 0 {
                let [low, high] = model.to_le_bytes();
                block.set_byte(protocol::OFF_MODEL, low);
                block.set_byte(protocol::OFF_MODEL + 1, high);
                touched.push((protocol::OFF_MODEL, low));
                touched.push((protocol::OFF_MODEL + 1, high));
            }
        }
        for (offset, value) in touched {
            self.remember_live_edit(kind, offset, value);
        }
    }

    /// Keep one local live-buffer byte from being pulled back by an older dump.
    fn remember_live_edit(&mut self, kind: Kind, offset: usize, value: u8) {
        if !self.connected || self.selected.0 != BANK_LIVE || offset < protocol::OFF_MODEL {
            return;
        }
        self.pending_live.insert(
            (kind.code(), offset),
            PendingLive {
                value,
                seen: Instant::now(),
            },
        );
    }

    /// Record everything a model load or reset changed in the live buffer.
    fn remember_live_preset(&mut self, before: Option<&protocol::Preset>) {
        if !self.connected || self.selected.0 != BANK_LIVE {
            return;
        }
        let Some(before) = before else {
            return;
        };
        let changes = {
            let Some(after) = self.dump.get(BANK_LIVE, 0) else {
                return;
            };
            protocol::diff_presets(before, after)
        };
        let seen = Instant::now();
        for change in changes {
            if change.offset >= protocol::OFF_MODEL {
                self.pending_live.insert(
                    (change.kind.code(), change.offset),
                    PendingLive {
                        value: change.after,
                        seen,
                    },
                );
            }
        }
    }

    /// Re-derive pending values after an undo, redo or other whole-dump edit.
    fn remember_live_diff(&mut self) {
        if !self.connected {
            return;
        }
        let changes = {
            let (Some(known), Some(current)) =
                (self.original.get(BANK_LIVE, 0), self.dump.get(BANK_LIVE, 0))
            else {
                return;
            };
            protocol::diff_presets(known, current)
        };
        let seen = Instant::now();
        for change in changes {
            if change.offset >= protocol::OFF_MODEL {
                self.pending_live.insert(
                    (change.kind.code(), change.offset),
                    PendingLive {
                        value: change.after,
                        seen,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Message, Preset, AMP_TO_HOST, FRAME_MARKER, REPORT_SIZE};

    fn amp_app(model: u16, voice: u8) -> App {
        let ctx = eframe::egui::Context::default();
        crate::ui::theme::install(&ctx);
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = Kind::Amp.code();
        raw[6] = 1;
        let [low, high] = model.to_le_bytes();
        raw[protocol::OFF_MODEL] = low;
        raw[protocol::OFF_MODEL + 1] = high;
        raw[44..=46].fill(voice);
        let mut live = Preset::new(BANK_LIVE, 0);
        live.add(Message::from_bytes(raw).expect("a valid amp block"));
        let mut app = App::fresh(ctx);
        app.dump.insert(live.clone());
        app.original.insert(live);
        app.selected = (BANK_LIVE, 0);
        app.connected = true;
        app.synced = true;
        app
    }

    fn channel_one_app() -> App {
        amp_app(models::CLEAN_CHANNEL_MODEL, 0)
    }

    #[test]
    fn channel_one_refuses_a_stompbox_model() {
        let mut app = channel_one_app();
        assert!(app.on_clean_channel());
        app.set_effect_model(Kind::Stomp, 0x00ba);
        let model = app
            .dump
            .get(BANK_LIVE, 0)
            .and_then(|preset| preset.block(Kind::Stomp))
            .and_then(|block| block.model());
        assert_eq!(model, None, "the amp would ignore it on channel 1");
        assert!(
            app.error
                .as_deref()
                .unwrap_or_default()
                .contains("Clean Channel"),
            "the error must name the channel"
        );
    }

    #[test]
    fn channel_one_refuses_an_amp_control() {
        let mut app = channel_one_app();
        app.apply_edit(Kind::Amp, protocol::OFF_MODEL, 0x67);
        let model = app
            .dump
            .get(BANK_LIVE, 0)
            .and_then(|preset| preset.block(Kind::Amp))
            .and_then(|block| block.model());
        assert_eq!(model, Some(models::CLEAN_CHANNEL_MODEL));
        assert!(app.error.is_some());
    }

    #[test]
    fn a_voice_edit_sends_the_model_and_voice_bytes_together() {
        let mut app = amp_app(0x0079, 0x0b); // British '70s
        let (tx, rx) = std::sync::mpsc::channel();
        app.tx = Some(tx);
        app.apply_edit(Kind::Amp, 44, 3); // '65 Deluxe
        app.send_live_edits();

        let frame = match rx.try_recv() {
            Ok(Cmd::Block { code, frame }) if code == Kind::Amp.code() => frame,
            Ok(_) => panic!("a block write was expected"),
            Err(error) => panic!("no block write was sent: {error:?}"),
        };
        assert_eq!(frame[1], protocol::HOST_CONTROL);
        assert_eq!(
            &frame[protocol::OFF_MODEL..protocol::OFF_MODEL + 2],
            &[0x53, 0x00]
        );
        assert_eq!(&frame[44..=46], &[3, 3, 3]);
    }

    #[test]
    fn channel_one_still_loads_the_effects_section() {
        let mut app = channel_one_app();
        app.set_effect_model(Kind::Reverb, 0x003a);
        let model = app
            .dump
            .get(BANK_LIVE, 0)
            .and_then(|preset| preset.block(Kind::Reverb))
            .and_then(|block| block.model());
        assert_eq!(
            model,
            Some(0x003a),
            "mod, delay and reverb work on channel 1"
        );
        assert!(app.error.is_none());
    }
}
