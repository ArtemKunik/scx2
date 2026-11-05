//! Merging what the amp reports into the app's state.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::params;
use crate::protocol::{self, Dump, Kind, Preset, BANK_LIVE};

use super::dial::scale10;
use super::edits::PENDING_TTL;
use super::App;

const LOG_LINES: usize = 8;
/// How long a changed parameter glows in the grid, in seconds.
const FLASH: f32 = 1.4;

impl App {
    /// The amp's full state. The live buffer stays on screen unless the person
    /// chose a stored preset, and a value being dragged is never pulled away.
    pub(crate) fn on_dump(&mut self, mut dump: Dump) {
        let held = self.held_value();
        let reported = dump.live().cloned();
        if let Some(live) = &reported {
            self.note_live_changes(live);
        }
        // A dump can arrive before the amp has picked up a local write. Lay
        // those local values back over its older copy of the live buffer.
        let pending = self.live_pending_values();
        overlay_pending(&mut dump, &pending);
        if let Some((offset, value)) = held {
            set_live_byte(&mut dump, offset, value);
        }
        self.original = dump.clone();
        if let Some(reported) = reported {
            if let Some(slot) = self.original.get_mut(BANK_LIVE, 0) {
                *slot = reported;
            }
        }
        self.dump = dump;
        if !self.synced || self.dump.get(self.selected.0, self.selected.1).is_none() {
            self.selected = (BANK_LIVE, 0);
        }
        self.synced = true;
        self.synced_at = Some(Instant::now());
    }

    /// The live buffer alone, available long before the rest of a dump.
    pub(crate) fn on_live(&mut self, mut live: Preset) {
        let reported = live.clone();
        self.note_live_changes(&reported);
        let pending = self.live_pending_values();
        overlay_preset(&mut live, &pending);
        if let Some((offset, value)) = self.held_value() {
            if let Some(block) = live.block_mut(Kind::Amp) {
                block.set_byte(offset, value);
            }
        }
        if let Some(slot) = self.original.get_mut(BANK_LIVE, 0) {
            *slot = reported;
        }
        if let Some(slot) = self.dump.get_mut(BANK_LIVE, 0) {
            *slot = live;
        }
    }

    /// A knob on the amp moved: the panel follows the hand on the knob.
    pub(crate) fn on_knob(&mut self, offset: usize, value: u8) {
        if self.active_offset == Some(offset) {
            return;
        }
        let key = (Kind::Amp.code(), offset);
        if let Some(pending) = self.pending_live.get(&key) {
            if pending.seen.elapsed() < PENDING_TTL {
                // A write can make the amp echo an older value before it
                // catches up. Do not let that pull the knob back.
                if pending.value != value {
                    return;
                }
            } else {
                self.pending_live.remove(&key);
            }
        }
        let before = self
            .dump
            .live()
            .and_then(|live| live.block(Kind::Amp))
            .map(|block| block.as_bytes()[offset]);
        if let Some(before) = before.filter(|before| *before != value) {
            self.note_change(offset, before, value);
        }
        set_live_byte(&mut self.dump, offset, value);
        set_live_byte(&mut self.original, offset, value);
        self.selected = (BANK_LIVE, 0);
    }

    /// The local values still waiting for the amp, after dropping stale ones.
    fn live_pending_values(&mut self) -> BTreeMap<(u8, usize), u8> {
        let now = Instant::now();
        self.pending_live
            .retain(|_, pending| now.duration_since(pending.seen) < PENDING_TTL);
        self.pending_live
            .iter()
            .map(|((code, offset), pending)| ((*code, *offset), pending.value))
            .collect()
    }

    fn held_value(&self) -> Option<(usize, u8)> {
        let offset = self.active_offset?;
        let block = self.dump.live()?.block(Kind::Amp)?;
        Some((offset, block.as_bytes()[offset]))
    }

    fn note_live_changes(&mut self, incoming: &Preset) {
        let changes = match self.original.live() {
            Some(known) if self.synced => protocol::diff_presets(known, incoming),
            _ => Vec::new(),
        };
        for change in changes {
            let is_param = change.kind == Kind::Amp && change.offset >= protocol::OFF_PARAMS;
            if is_param && self.active_offset != Some(change.offset) {
                self.note_change(change.offset, change.before, change.after);
            }
        }
    }

    fn note_change(&mut self, offset: usize, before: u8, after: u8) {
        self.flash.insert(offset, FLASH);
        record_change(&mut self.activity, offset, before, after);
    }
}

fn set_live_byte(dump: &mut Dump, offset: usize, value: u8) {
    if let Some(block) = dump
        .get_mut(BANK_LIVE, 0)
        .and_then(|preset| preset.block_mut(Kind::Amp))
    {
        block.set_byte(offset, value);
    }
}

/// Put local values back over the amp's copy of the live buffer.
fn overlay_pending(dump: &mut Dump, pending: &BTreeMap<(u8, usize), u8>) {
    if let Some(live) = dump.get_mut(BANK_LIVE, 0) {
        overlay_preset(live, pending);
    }
}

fn overlay_preset(live: &mut Preset, pending: &BTreeMap<(u8, usize), u8>) {
    for ((code, offset), value) in pending {
        if let Some(block) = live.ensure_block(Kind::from_code(*code)) {
            block.set_byte(*offset, *value);
        }
    }
}

/// One entry in the recent changes list.
pub(crate) struct Activity {
    pub(crate) offset: usize,
    pub(crate) from: u8,
    pub(crate) to: u8,
}

impl Activity {
    pub(crate) fn line(&self) -> String {
        let name = params::offset_title(self.offset).unwrap_or_else(|| "Unnamed".into());
        format!(
            "{name:<16} offset {:<3} {} -> {}",
            self.offset,
            scale10(self.from),
            scale10(self.to)
        )
    }
}

/// A sweep of one knob reads as a single entry, not dozens.
pub(crate) fn record_change(log: &mut Vec<Activity>, offset: usize, before: u8, after: u8) {
    if let Some(last) = log.last_mut().filter(|last| last.offset == offset) {
        last.to = after;
        return;
    }
    log.push(Activity {
        offset,
        from: before,
        to: after,
    });
    if log.len() > LOG_LINES {
        log.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::REPORT_SIZE;
    use crate::ui::worker::apply_newer;

    fn dump_with_live_volume(value: u8) -> Dump {
        let mut report = [0u8; REPORT_SIZE];
        report[0] = protocol::FRAME_MARKER;
        report[1] = protocol::AMP_TO_HOST;
        report[2] = Kind::Amp.code();
        report[6] = 1;
        report[32] = value;
        let mut dump = Dump::new();
        dump.add_raw(&report);
        dump
    }

    fn live_volume(dump: &Dump) -> u8 {
        dump.live().unwrap().block(Kind::Amp).unwrap().as_bytes()[32]
    }

    #[test]
    fn a_knob_sweep_is_one_log_entry() {
        let mut log = Vec::new();
        record_change(&mut log, 32, 0x10, 0x11);
        record_change(&mut log, 32, 0x11, 0x12);
        record_change(&mut log, 33, 0x20, 0x21);
        assert_eq!(log.len(), 2);
        assert_eq!((log[0].from, log[0].to), (0x10, 0x12));
    }

    #[test]
    fn the_log_stays_short() {
        let mut log = Vec::new();
        for offset in 32..60 {
            record_change(&mut log, offset, 0, 1);
        }
        assert_eq!(log.len(), LOG_LINES);
        assert_eq!(log.last().unwrap().offset, 59);
    }

    #[test]
    fn a_knob_moved_during_a_dump_beats_the_dumps_older_copy() {
        let mut dump = dump_with_live_volume(0x10);
        apply_newer(&mut dump, &BTreeMap::from([(32, 0x40)]));
        assert_eq!(live_volume(&dump), 0x40);
    }

    #[test]
    fn set_live_byte_only_touches_the_live_buffer() {
        let mut dump = dump_with_live_volume(0x10);
        set_live_byte(&mut dump, 32, 0x7f);
        assert_eq!(live_volume(&dump), 0x7f);
    }

    #[test]
    fn a_pending_local_value_beats_an_older_dump() {
        let mut dump = dump_with_live_volume(0x10);
        overlay_pending(&mut dump, &BTreeMap::from([((Kind::Amp.code(), 32), 0x40)]));
        assert_eq!(
            live_volume(&dump),
            0x40,
            "the local value must survive the dump"
        );
    }
}
