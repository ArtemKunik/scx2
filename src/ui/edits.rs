//! Applying edits locally and pushing them to the amp.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::params;
use crate::protocol::{self, Kind};

use super::worker::Frame;
use super::App;

impl App {
    pub(crate) fn pending(&self) -> Vec<protocol::Change> {
        protocol::diff_dumps(&self.original, &self.dump)
    }

    pub(crate) fn write_pending(&mut self) {
        let mut blocks: BTreeSet<(u8, u8, u8)> = BTreeSet::new();
        let mut frames: Vec<Frame> = Vec::new();

        for change in self.pending() {
            // Parameters the amp drives through direct control ignore block
            // writes; send them the way the front-panel knob does.
            if change.kind == Kind::Amp {
                if let Some(param) = params::index_for_offset(change.offset) {
                    let model = self
                        .dump
                        .get(change.bank, change.index)
                        .and_then(|preset| preset.block(Kind::Amp))
                        .and_then(|block| block.model())
                        .unwrap_or(0);
                    frames.push((
                        params::direct_control(
                            model,
                            param,
                            params::SELECTOR_AMP,
                            params::magic_from_byte(change.after),
                        ),
                        false,
                    ));
                    continue;
                }
            }
            blocks.insert((change.bank, change.index, change.kind.code()));
        }

        for (bank, index, code) in blocks {
            if let Some(message) = self
                .dump
                .get(bank, index)
                .and_then(|preset| preset.block(Kind::from_code(code)))
            {
                frames.push((
                    protocol::as_host_frame(message.as_bytes(), protocol::HOST_CONTROL),
                    true,
                ));
            }
        }

        if frames.is_empty() {
            return;
        }
        match &self.tx {
            Some(tx) if self.connected => {
                let _ = tx.send(super::worker::Cmd::Write(frames.clone()));
                self.status = format!("sending {} report(s)...", frames.len());
            }
            _ => {
                self.error = Some(
                    "no amp connected: the changes are kept locally, connect to send them".into(),
                );
            }
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
            Ok(()) => self.status = format!("saved {}", path.display()),
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(crate) fn apply_edit(&mut self, kind: Kind, offset: usize, value: u8) {
        let (bank, index) = self.selected;
        if let Some(preset) = self.dump.get_mut(bank, index) {
            if let Some(block) = preset.block_mut(kind) {
                block.set_byte(offset, value);
            }
        }
        // Voice is stored three times; keep the copies in sync.
        if kind == Kind::Amp && (44..=46).contains(&offset) {
            if let Some(preset) = self.dump.get_mut(bank, index) {
                if let Some(block) = preset.block_mut(Kind::Amp) {
                    for other in [44usize, 45, 46] {
                        block.set_byte(other, value);
                    }
                }
            }
        }
    }
}
