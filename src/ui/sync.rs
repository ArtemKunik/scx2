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
    ///
    /// A dump is not allowed to undo a local edit that the amp has not caught
    /// up with yet. That matters twice: the person keeps seeing what they set,
    /// and the app does not mistake the amp's older copy for an unsent change
    /// and write it again on every dump.
    pub(crate) fn on_dump(&mut self, mut dump: Dump) {
        let first = !self.synced;
        let reported = dump.live().cloned();
        // The last state we sent or accepted, kept as the baseline for live
        // edits. The amp's copy only replaces the bytes nothing local covers.
        let saved = self.original.clone();
        let baseline_live = saved.live().cloned();

        if !first {
            // Preset Info lives beside the device bytes. Keep it across dumps;
            // the amp never reports it and would otherwise erase it.
            copy_info(&mut dump, &self.dump);
        }
        self.original = dump.clone();
        self.dump = dump;
        if !first {
            // The library metadata is not the amp's, so the baseline keeps the
            // last saved copy. That keeps "unsaved info" visible as a change.
            for preset in self.original.presets.values_mut() {
                if let Some(old) = saved.get(preset.bank, preset.index) {
                    preset.info = old.info.clone();
                }
            }
            if let Some(reported) = reported {
                if let Some(baseline) = baseline_live {
                    self.original.insert(baseline);
                }
                self.accept_live(&reported);
            }
        }
        if first || self.dump.get(self.selected.0, self.selected.1).is_none() {
            self.selected = (BANK_LIVE, 0);
        }
        self.synced = true;
        self.synced_at = Some(Instant::now());
    }

    /// The live buffer alone, available long before the rest of a dump.
    pub(crate) fn on_live(&mut self, live: Preset) {
        if !self.synced {
            self.dump.insert(live.clone());
            self.original.insert(live);
            return;
        }
        self.accept_live(&live);
    }

    /// Fold the amp's live buffer into the one on screen.
    ///
    /// A pending local byte stays until the amp reports the same value, or
    /// until [`PENDING_TTL`] runs out. Every other byte follows the amp, so a
    /// front-panel knob still moves the panel. Confirmed values clear their
    /// pending entry; a value the amp never takes is reported once.
    fn accept_live(&mut self, incoming: &Preset) {
        self.expire_pending(incoming);
        self.note_live_changes(incoming);

        let pending: BTreeMap<(u8, usize), u8> = self
            .pending_live
            .iter()
            .map(|(key, value)| (*key, value.value))
            .collect();

        if self.original.get(BANK_LIVE, 0).is_none() {
            self.original.insert(incoming.clone());
        }
        let Some(display) = self.dump.get_mut(BANK_LIVE, 0) else {
            return;
        };
        display.name = incoming.name.clone();
        let Some(baseline) = self.original.get_mut(BANK_LIVE, 0) else {
            return;
        };

        let mut confirmed = Vec::new();
        for block in incoming.blocks.values() {
            let kind = block.kind();
            let code = kind.code();
            let (Some(target), Some(base)) =
                (display.ensure_block(kind), baseline.ensure_block(kind))
            else {
                continue;
            };
            for offset in 0..protocol::REPORT_SIZE {
                let amp = block.as_bytes()[offset];
                if let Some(local) = pending.get(&(code, offset)) {
                    // The local value is what was sent. Keep it in both the
                    // panel and the baseline until the amp confirms it.
                    target.set_byte(offset, *local);
                    base.set_byte(offset, *local);
                    if amp != *local {
                        continue;
                    }
                    confirmed.push((code, offset));
                }
                target.set_byte(offset, amp);
                base.set_byte(offset, amp);
            }
        }
        for key in confirmed {
            self.pending_live.remove(&key);
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

    /// Drop local values the amp has had long enough to accept.
    ///
    /// If the amp's copy still differs, the write did not take. Report that
    /// once instead of keeping the panel on a value the amp is not playing.
    fn expire_pending(&mut self, incoming: &Preset) {
        let now = Instant::now();
        let expired: Vec<_> = self
            .pending_live
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.seen) >= PENDING_TTL)
            .map(|(key, pending)| (*key, pending.value))
            .collect();
        if expired.is_empty() {
            return;
        }
        let mut rejected = None;
        for (key, value) in expired {
            self.pending_live.remove(&key);
            let amp = incoming
                .blocks
                .get(&key.0)
                .map(|block| block.as_bytes()[key.1]);
            // The whole amp panel is snapshotted around a block write, so a
            // difference there can be a stale echo or a hand on a front-panel
            // knob. Only model/slot/effect changes are worth a warning.
            let structural = key.0 != Kind::Amp.code() || key.1 < protocol::OFF_PARAMS;
            if structural && amp.is_some_and(|amp| amp != value) {
                rejected.get_or_insert(key);
            }
        }
        if let Some(key) = rejected {
            self.note_rejected(key);
        }
    }

    fn note_rejected(&mut self, key: (u8, usize)) {
        let kind = Kind::from_code(key.0);
        if self.on_clean_channel() && matches!(kind, Kind::Amp | Kind::Stomp) {
            self.error = Some(
                "The amp did not keep that change: it reports the Clean Channel. \
                 Switch to Channel 2 (and turn its Voice knob off Clean Channel), then try again."
                    .into(),
            );
        } else {
            self.status = format!("The amp did not keep the {} change.", kind.label());
        }
    }

    fn note_live_changes(&mut self, incoming: &Preset) {
        if !self.synced {
            return;
        }
        let Some(known) = self.dump.live().cloned() else {
            return;
        };
        for change in protocol::diff_presets(&known, incoming) {
            if self
                .pending_live
                .contains_key(&(change.kind.code(), change.offset))
            {
                continue;
            }
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

/// Keep the editor's library metadata when the amp's dump replaces the bytes.
fn copy_info(target: &mut Dump, source: &Dump) {
    for preset in target.presets.values_mut() {
        if let Some(old) = source.get(preset.bank, preset.index) {
            preset.info = old.info.clone();
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
    use eframe::egui;

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

    fn live_preset(value: u8) -> Preset {
        dump_with_live_volume(value).live().unwrap().clone()
    }

    fn live_app(volume: u8) -> App {
        let ctx = egui::Context::default();
        crate::ui::theme::install(&ctx);
        let mut app = App::fresh(ctx);
        app.dump = dump_with_live_volume(volume);
        app.original = app.dump.clone();
        app.selected = (BANK_LIVE, 0);
        app.synced = true;
        app.connected = true;
        app
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
        let mut app = live_app(0x40);
        app.pending_live.insert(
            (Kind::Amp.code(), 32),
            crate::ui::app::PendingLive {
                value: 0x40,
                seen: Instant::now(),
            },
        );
        app.accept_live(&live_preset(0x10));
        assert_eq!(
            live_volume(&app.dump),
            0x40,
            "the local value must survive the dump"
        );
        assert_eq!(
            live_volume(&app.original),
            0x40,
            "the baseline must stay on the value that was sent"
        );
    }

    #[test]
    fn a_dump_that_confirms_a_value_clears_the_pending_entry() {
        let mut app = live_app(0x40);
        app.pending_live.insert(
            (Kind::Amp.code(), 32),
            crate::ui::app::PendingLive {
                value: 0x40,
                seen: Instant::now(),
            },
        );
        app.accept_live(&live_preset(0x40));
        assert!(
            app.pending_live.is_empty(),
            "a confirmed value is no longer pending"
        );
        assert_eq!(live_volume(&app.dump), 0x40);
        assert_eq!(live_volume(&app.original), 0x40);
    }

    #[test]
    fn a_dump_does_not_turn_a_sent_value_into_a_resend() {
        let mut app = live_app(0x40);
        app.pending_live.insert(
            (Kind::Amp.code(), 32),
            crate::ui::app::PendingLive {
                value: 0x40,
                seen: Instant::now(),
            },
        );
        // The amp still reports the value from before the write.
        app.accept_live(&live_preset(0x10));
        assert!(
            app.pending().is_empty(),
            "the amp's older copy must not look like an unsent edit"
        );
    }

    #[test]
    fn a_dump_does_not_replace_a_pending_local_value() {
        let mut app = live_app(0x40);
        app.pending_live.insert(
            (Kind::Amp.code(), 32),
            crate::ui::app::PendingLive {
                value: 0x40,
                seen: Instant::now(),
            },
        );
        // The full-dump path replaces the live preset with the amp's copy
        // before merging; the pending value must be laid back over it.
        app.on_dump(dump_with_live_volume(0x10));
        assert_eq!(
            live_volume(&app.dump),
            0x40,
            "a full dump must not replace the local value"
        );
        assert!(
            app.pending().is_empty(),
            "and it must not look like an unsent edit either"
        );
    }

    #[test]
    fn a_pending_value_the_amp_never_takes_follows_the_amp_after_the_grace() {
        let mut app = live_app(0x40);
        app.pending_live.insert(
            (Kind::Amp.code(), 32),
            crate::ui::app::PendingLive {
                value: 0x40,
                seen: Instant::now() - PENDING_TTL - std::time::Duration::from_secs(1),
            },
        );
        app.accept_live(&live_preset(0x10));
        assert!(app.pending_live.is_empty());
        assert_eq!(
            live_volume(&app.dump),
            0x10,
            "once the grace runs out, the amp's copy wins"
        );
    }

    #[test]
    fn a_model_change_the_amp_rejects_is_reported() {
        let mut app = live_app(0x40);
        for dump in [&mut app.dump, &mut app.original] {
            if let Some(block) = dump
                .get_mut(BANK_LIVE, 0)
                .and_then(|preset| preset.ensure_block(Kind::Stomp))
            {
                block.set_byte(protocol::OFF_MODEL, 0xba);
                block.set_byte(protocol::OFF_MODEL + 1, 0);
            }
        }
        app.pending_live.insert(
            (Kind::Stomp.code(), protocol::OFF_MODEL),
            crate::ui::app::PendingLive {
                value: 0xba,
                seen: Instant::now() - PENDING_TTL - std::time::Duration::from_secs(1),
            },
        );
        let mut incoming = live_preset(0x40);
        incoming.ensure_block(Kind::Stomp);
        app.accept_live(&incoming);
        assert!(
            app.status.contains("stomp"),
            "a rejected model change must be named: {:?}",
            app.status
        );
    }

    #[test]
    fn amp_dumps_keep_the_library_metadata() {
        let mut app = live_app(0x10);
        let mut stored = Preset::new(crate::protocol::BANK_AMP, 1);
        stored.info.description = "my notes".into();
        app.dump.insert(stored.clone());
        app.original.insert(stored);
        app.selected = (crate::protocol::BANK_AMP, 1);

        let mut incoming = dump_with_live_volume(0x10);
        incoming.insert(Preset::new(crate::protocol::BANK_AMP, 1));
        app.on_dump(incoming);

        assert_eq!(
            app.dump
                .get(crate::protocol::BANK_AMP, 1)
                .unwrap()
                .info
                .description,
            "my notes",
            "the amp never reports Preset Info, so the dump must keep it"
        );
        assert_eq!(app.selected, (crate::protocol::BANK_AMP, 1));
    }
}
