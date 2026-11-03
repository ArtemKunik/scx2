//! Applying edits locally and sending live ones to the amp.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::protocol::{self, Kind, BANK_LIVE};
use crate::{models, params};

use super::worker::Cmd;
use super::App;

impl App {
    pub(crate) fn pending(&self) -> Vec<protocol::Change> {
        protocol::diff_dumps(&self.original, &self.dump)
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
        match serde_json::to_string_pretty(&self.dump.to_json())
            .map_err(anyhow::Error::from)
            .and_then(|text| std::fs::write(&path, text).map_err(anyhow::Error::from))
        {
            Ok(()) => self.status = format!("Saved {}", path.display()),
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(crate) fn apply_edit(&mut self, kind: Kind, offset: usize, value: u8) {
        let (bank, index) = self.selected;
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
