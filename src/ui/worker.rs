//! Background thread that owns the HID device and keeps the app in sync.
//!
//! It never blocks on a dump: a dump takes about two seconds, and knob pushes
//! and writes have to keep flowing while one is running. One loop therefore
//! sends what is queued, handles one report, and starts a dump when one is due.

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui;

use crate::params;
use crate::protocol::{self, Dump, Kind, Notice, Preset, BANK_LIVE};
use crate::transport::Amp;

/// Fallback for anything the amp does not announce.
const RESYNC: Duration = Duration::from_secs(5);
/// Do not start a routine dump while someone is turning a knob.
const QUIET: Duration = Duration::from_secs(1);
/// Give up waiting for a write's acknowledgement after this long.
const ACK_WAIT: Duration = Duration::from_millis(150);
const BLOCK_GAP: Duration = Duration::from_millis(60);
const SOON: Duration = Duration::from_millis(150);
const DUMP_LIMIT: Duration = Duration::from_secs(10);
/// A real dump has 34 presets; anything far below that was cut short.
const COMPLETE_DUMP: usize = 20;

pub(crate) enum Cmd {
    /// Set one amp parameter through direct control.
    SetParam { model: u16, index: u8, value: u8 },
    /// Replace one block of the live buffer; only the newest per kind matters.
    Block {
        code: u8,
        frame: [u8; protocol::REPORT_SIZE],
    },
    /// FUSE's fast bypass frame; keeps the model and its settings.
    Bypass {
        kind: Kind,
        slot: u8,
        bypassed: bool,
    },
    /// Save the live buffer into a slot with FUSE's save-preset frame.
    Store { bank: u8, index: u8, name: String },
    /// Rewrite a stored slot's name without touching its sound.
    Rename { bank: u8, index: u8, name: String },
    /// Dump as soon as the write queue is empty, e.g. to read a target slot.
    Refresh,
}

pub(crate) enum Ev {
    Connected(Option<String>),
    Dump(Dump),
    /// The live buffer, sent as soon as a dump has passed it.
    Live(Box<Preset>),
    Knob {
        offset: usize,
        value: u8,
    },
    /// A store was sent; `outcome` carries the read-back result.
    StoreDone {
        bank: u8,
        index: u8,
        outcome: Result<String, String>,
    },
    /// A rename was sent; `outcome` carries the read-back result.
    RenameDone {
        bank: u8,
        index: u8,
        outcome: Result<String, String>,
    },
    Warning(String),
    Error(String),
}

/// A device write queued for the worker, in request order.
enum DeviceWrite {
    Store { bank: u8, index: u8, name: String },
    Rename { bank: u8, index: u8, name: String },
}

/// A write whose result is checked against the next complete dump.
enum Verify {
    Store {
        bank: u8,
        index: u8,
        name: String,
    },
    Rename {
        bank: u8,
        index: u8,
        name: String,
        /// The slot as it was before the rename, for the byte comparison.
        before: Option<Preset>,
    },
}

pub(crate) fn spawn_worker(
    path: Option<String>,
    tx: Sender<Ev>,
    rx: Receiver<Cmd>,
    ctx: egui::Context,
) {
    // The device is opened inside the thread, so nothing has to be Send.
    thread::spawn(move || {
        let send = |event: Ev| {
            let _ = tx.send(event);
            ctx.request_repaint();
        };
        let outcome = Amp::open(path.as_deref()).and_then(|amp| {
            let banner = amp.handshake().ok().flatten();
            send(Ev::Connected(banner));
            run(&amp, &send, &rx)
        });
        if let Err(error) = outcome {
            send(Ev::Error(format!("{error:#}")));
        }
    });
}

/// A dump in progress, and what has changed since it read the live buffer.
struct Collecting {
    dump: Dump,
    started: Instant,
    live_sent: bool,
    /// Newer than the dump's copy of the live amp block.
    newer: BTreeMap<usize, u8>,
    /// A whole block was written, so this dump cannot be trusted.
    stale: bool,
}

fn run(amp: &Amp, send: &impl Fn(Ev), rx: &Receiver<Cmd>) -> Result<()> {
    let mut params_out: BTreeMap<u8, (u16, u8)> = BTreeMap::new();
    let mut blocks_out = BTreeMap::new();
    let mut bypasses_out = BTreeMap::new();
    let mut device_out: VecDeque<DeviceWrite> = VecDeque::new();
    // The last dump sent to the UI; a rename compares its slot against it.
    let mut last_dump: Option<Dump> = None;
    // A write whose read-back waits for the next complete dump.
    let mut verify: Option<Verify> = None;
    let mut awaiting_ack: Option<Instant> = None;
    let mut last_block = Instant::now() - BLOCK_GAP;
    let mut last_activity = Instant::now() - QUIET;
    // Our own block/bypass write can make the amp announce a state change.
    // Do not answer that with a dump; the live buffer on screen is already
    // newer than the copy the amp would send back.
    let mut quiet_until = Instant::now() - QUIET;
    let mut collecting: Option<Collecting> = None;
    let mut dump_soon = Some(Instant::now());
    let mut next_resync = Instant::now() + RESYNC;

    loop {
        loop {
            match rx.try_recv() {
                Ok(Cmd::SetParam {
                    model,
                    index,
                    value,
                }) => {
                    params_out.insert(index, (model, value));
                }
                Ok(Cmd::Block { code, frame }) => {
                    blocks_out.insert(code, frame);
                }
                Ok(Cmd::Bypass {
                    kind,
                    slot,
                    bypassed,
                }) => {
                    bypasses_out.insert(kind.code(), (kind, slot, bypassed));
                }
                Ok(Cmd::Store { bank, index, name }) => {
                    device_out.push_back(DeviceWrite::Store { bank, index, name });
                }
                Ok(Cmd::Rename { bank, index, name }) => {
                    device_out.push_back(DeviceWrite::Rename { bank, index, name });
                }
                Ok(Cmd::Refresh) => {
                    dump_soon = Some(Instant::now());
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        let now = Instant::now();
        if awaiting_ack.is_some_and(|sent| now - sent >= ACK_WAIT) {
            awaiting_ack = None;
        }
        // One write at a time, as the amp answers each one.
        if awaiting_ack.is_none() {
            if let Some((index, (model, value))) = params_out.pop_first() {
                let wire = params::magic_from_byte(value);
                amp.write_report(&params::direct_control(
                    model,
                    index,
                    params::SELECTOR_AMP,
                    wire,
                ))?;
                awaiting_ack = Some(now);
                last_activity = now;
                if let (Some(dump), Some(offset)) =
                    (collecting.as_mut(), params::offset_for_index(index))
                {
                    dump.newer.insert(offset, value);
                }
            } else if now - last_block >= BLOCK_GAP {
                if let Some((_, (kind, slot, bypassed))) = bypasses_out.pop_first() {
                    amp.write_report(&protocol::bypass(kind, slot, bypassed))?;
                    last_block = now;
                    last_activity = now;
                    quiet_until = now + Duration::from_secs(1);
                    if let Some(dump) = collecting.as_mut() {
                        dump.stale = true;
                    }
                } else if let Some((_, frame)) = blocks_out.pop_first() {
                    amp.write_report(&frame)?;
                    // The amp buffers a block write until the execute frame.
                    amp.write_report(&protocol::execute_model_change())?;
                    last_block = now;
                    last_activity = now;
                    quiet_until = now + Duration::from_secs(1);
                    if let Some(dump) = collecting.as_mut() {
                        dump.stale = true;
                    }
                } else if let Some(write) = device_out.pop_front() {
                    // These frames read or rewrite stored memory, so they only
                    // go out once every queued live edit has landed.
                    match &write {
                        DeviceWrite::Store { bank, index, name } => {
                            amp.write_report(&protocol::save_preset(*bank, *index, name))?;
                            verify = Some(Verify::Store {
                                bank: *bank,
                                index: *index,
                                name: name.clone(),
                            });
                        }
                        DeviceWrite::Rename { bank, index, name } => {
                            let raw = protocol::name_change(*bank, *index, name);
                            amp.write_report(&protocol::as_host_frame(
                                &raw,
                                protocol::HOST_CONTROL,
                            ))?;
                            amp.write_report(&protocol::execute_model_change())?;
                            // The dump the UI is looking at is the "before"
                            // copy for the byte comparison.
                            let before = last_dump
                                .as_ref()
                                .and_then(|dump| dump.get(*bank, *index))
                                .cloned();
                            verify = Some(Verify::Rename {
                                bank: *bank,
                                index: *index,
                                name: name.clone(),
                                before,
                            });
                        }
                    }
                    last_block = now;
                    last_activity = now;
                    quiet_until = now + Duration::from_secs(1);
                    if let Some(dump) = collecting.as_mut() {
                        dump.stale = true;
                    }
                    // Let the amp write flash, then read the slot back.
                    dump_soon = Some(now + Duration::from_millis(400));
                }
            }
        }

        let busy = awaiting_ack.is_some()
            || !params_out.is_empty()
            || !blocks_out.is_empty()
            || !bypasses_out.is_empty()
            || !device_out.is_empty();
        if let Some(report) = amp.read_report(if busy { 2 } else { 15 })? {
            let now = Instant::now();
            match protocol::classify(&report) {
                Some(Notice::AmpParam { index, value, .. }) => {
                    if let Some(offset) = params::offset_for_index(index) {
                        send(Ev::Knob { offset, value });
                        last_activity = now;
                        if let Some(dump) = collecting.as_mut() {
                            dump.newer.insert(offset, value);
                        }
                    }
                }
                Some(Notice::Ack { status }) => {
                    awaiting_ack = None;
                    if status != 0 {
                        send(Ev::Warning(format!(
                            "The amp rejected a change (status {status})."
                        )));
                    }
                }
                // Dumps end with these same reports, so only react between
                // dumps. Ignore the announcement that our own write causes.
                Some(Notice::StateChanged) if collecting.is_none() && now >= quiet_until => {
                    dump_soon = Some(now + SOON);
                }
                _ => {}
            }

            if let Some(state) = collecting.as_mut() {
                state.dump.add_raw(&report);
                let past_live = report[0] == protocol::FRAME_MARKER && report[3] != BANK_LIVE;
                if past_live && !state.live_sent {
                    state.live_sent = true;
                    apply_newer(&mut state.dump, &state.newer);
                    if let Some(live) = state.dump.live() {
                        send(Ev::Live(Box::new(live.clone())));
                    }
                }
                if state.dump.ended {
                    next_resync = now + RESYNC;
                    if let Some(mut done) = collecting.take() {
                        if done.stale || done.dump.presets().count() < COMPLETE_DUMP {
                            dump_soon = Some(now + SOON);
                        } else {
                            apply_newer(&mut done.dump, &done.newer);
                            // A dump started before a write is discarded above,
                            // so this one is the read-back.
                            match verify.take() {
                                Some(Verify::Store { bank, index, name }) => {
                                    let outcome = verify_store(&done.dump, bank, index, &name);
                                    send(Ev::StoreDone {
                                        bank,
                                        index,
                                        outcome,
                                    });
                                }
                                Some(Verify::Rename {
                                    bank,
                                    index,
                                    name,
                                    before,
                                }) => {
                                    let outcome = verify_rename(
                                        &done.dump,
                                        before.as_ref(),
                                        bank,
                                        index,
                                        &name,
                                    );
                                    send(Ev::RenameDone {
                                        bank,
                                        index,
                                        outcome,
                                    });
                                }
                                None => {}
                            }
                            last_dump = Some(done.dump.clone());
                            send(Ev::Dump(done.dump));
                        }
                    }
                }
            }
        }

        let now = Instant::now();
        if collecting
            .as_ref()
            .is_some_and(|state| now - state.started >= DUMP_LIMIT)
        {
            collecting = None;
            dump_soon = Some(now + SOON);
        }
        let idle = awaiting_ack.is_none()
            && params_out.is_empty()
            && blocks_out.is_empty()
            && bypasses_out.is_empty()
            && device_out.is_empty();
        let asked = dump_soon.is_some_and(|at| now >= at);
        let routine = now >= next_resync && now - last_activity >= QUIET;
        if collecting.is_none() && idle && (asked || routine) {
            amp.write_report(&protocol::request_dump())?;
            dump_soon = None;
            collecting = Some(Collecting {
                dump: Dump::new(),
                started: now,
                live_sent: false,
                newer: BTreeMap::new(),
                stale: false,
            });
        }
    }
}

/// Lay values that arrived during a dump over its older copy of the live block.
pub(crate) fn apply_newer(dump: &mut Dump, newer: &BTreeMap<usize, u8>) {
    let block = dump
        .get_mut(BANK_LIVE, 0)
        .and_then(|preset| preset.block_mut(Kind::Amp));
    if let Some(block) = block {
        for (offset, value) in newer {
            block.set_byte(*offset, *value);
        }
    }
}

/// Compare a stored slot with the live buffer it was copied from.
///
/// Only the payload is compared: the header carries the bank and index, and
/// the amp flips byte 7 after the execute frame. The name is compared too,
/// because a save frame carries it beside the buffer.
fn verify_store(dump: &Dump, bank: u8, index: u8, name: &str) -> Result<String, String> {
    let Some(stored) = dump.get(bank, index) else {
        return Err(format!(
            "The amp did not report {} {index} after the save.",
            protocol::bank_label(bank)
        ));
    };
    let Some(live) = dump.live() else {
        return Err("The amp did not report its live buffer after the save.".into());
    };

    let mut problems = Vec::new();
    if stored.name != name.trim() {
        problems.push(format!("the name reads {:?}", stored.name));
    }
    let changes = protocol::diff_payload(live, stored);
    if let Some(first) = changes.first() {
        problems.push(format!(
            "{} byte(s) differ, first at {} offset {} (0x{:02x} -> 0x{:02x})",
            changes.len(),
            first.kind.label(),
            first.offset,
            first.before,
            first.after
        ));
    }
    for (kind, model) in protocol::models_missing_from(live, stored) {
        problems.push(format!(
            "the {} model 0x{model:04x} is missing",
            kind.label()
        ));
    }

    if problems.is_empty() {
        Ok(format!(
            "Saved {:?} to {} {index} · read-back matches",
            name.trim(),
            protocol::bank_label(bank)
        ))
    } else {
        Err(format!(
            "Saved, but the read-back does not match: {}.",
            problems.join("; ")
        ))
    }
}

/// Check that a rename changed the name and nothing else.
///
/// `before` is the slot from the dump the UI was looking at; it may be missing
/// if no full dump had arrived before the rename, in which case only the name
/// can be confirmed.
fn verify_rename(
    dump: &Dump,
    before: Option<&Preset>,
    bank: u8,
    index: u8,
    name: &str,
) -> Result<String, String> {
    let Some(after) = dump.get(bank, index) else {
        return Err(format!(
            "The amp did not report {} {index} after the rename.",
            protocol::bank_label(bank)
        ));
    };

    let mut problems = Vec::new();
    if after.name != name.trim() {
        problems.push(format!("the name reads {:?}", after.name));
    }
    if let Some(before) = before {
        let changes = protocol::diff_payload(before, after);
        if let Some(first) = changes.first() {
            problems.push(format!(
                "{} payload byte(s) changed, first at {} offset {} (0x{:02x} -> 0x{:02x})",
                changes.len(),
                first.kind.label(),
                first.offset,
                first.before,
                first.after
            ));
        }
    }

    if problems.is_empty() {
        Ok(match before {
            Some(_) => format!(
                "Renamed {} {index} to {:?} · other bytes unchanged",
                protocol::bank_label(bank),
                name.trim()
            ),
            None => format!(
                "Renamed {} {index} to {:?} · name read back",
                protocol::bank_label(bank),
                name.trim()
            ),
        })
    } else {
        Err(format!(
            "Renamed, but the read-back does not match: {}.",
            problems.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AMP_TO_HOST, FRAME_MARKER, OFF_MODEL, REPORT_SIZE};

    fn report(kind: Kind, bank: u8, index: u8, model: u16, volume: u8) -> [u8; REPORT_SIZE] {
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = kind.code();
        raw[3] = bank;
        raw[4] = index;
        raw[6] = 1;
        let [low, high] = model.to_le_bytes();
        raw[OFF_MODEL] = low;
        raw[OFF_MODEL + 1] = high;
        raw[32] = volume;
        raw
    }

    fn dump_with_slot(slot_volume: u8) -> Dump {
        let mut dump = Dump::new();
        dump.add_raw(&protocol::name_change(BANK_LIVE, 0, "live"));
        dump.add_raw(&report(Kind::Amp, BANK_LIVE, 0, 0x21, 0x63));
        dump.add_raw(&protocol::name_change(protocol::BANK_AMP, 3, "My preset"));
        dump.add_raw(&report(Kind::Amp, protocol::BANK_AMP, 3, 0x21, slot_volume));
        dump
    }

    #[test]
    fn a_matching_read_back_is_reported_as_saved() {
        let dump = dump_with_slot(0x63);
        let outcome = verify_store(&dump, protocol::BANK_AMP, 3, "My preset");
        let message = outcome.expect("the slot matches the live buffer");
        assert!(message.contains("read-back matches"), "{message}");
    }

    #[test]
    fn a_changed_parameter_fails_the_read_back() {
        let dump = dump_with_slot(0x40);
        let error = verify_store(&dump, protocol::BANK_AMP, 3, "My preset")
            .expect_err("the slot kept an older volume");
        assert!(error.contains("offset 32"), "{error}");
    }

    #[test]
    fn a_missing_slot_is_an_error() {
        let dump = dump_with_slot(0x63);
        let error = verify_store(&dump, protocol::BANK_AMP, 9, "My preset")
            .expect_err("slot 9 was never saved");
        assert!(error.contains("did not report amp 9"), "{error}");
    }

    #[test]
    fn a_loaded_model_missing_from_the_slot_is_an_error() {
        let mut dump = dump_with_slot(0x63);
        dump.add_raw(&report(Kind::Stomp, BANK_LIVE, 0, 0x0042, 0x80));
        let error = verify_store(&dump, protocol::BANK_AMP, 3, "My preset")
            .expect_err("the stomp block never reached the slot");
        assert!(error.contains("stomp model 0x0042"), "{error}");
    }

    #[test]
    fn a_rename_that_only_changed_the_name_passes() {
        let dump = dump_with_slot(0x63);
        let before = dump.get(protocol::BANK_AMP, 3).cloned();
        let mut renamed = dump.clone();
        renamed
            .get_mut(protocol::BANK_AMP, 3)
            .expect("the slot exists")
            .set_name("New name");
        let message = verify_rename(&renamed, before.as_ref(), protocol::BANK_AMP, 3, "New name")
            .expect("only the name moved");
        assert!(message.contains("other bytes unchanged"), "{message}");
    }

    #[test]
    fn a_rename_that_moved_a_parameter_is_an_error() {
        let dump = dump_with_slot(0x63);
        let before = dump.get(protocol::BANK_AMP, 3).cloned();
        let mut after = dump.clone();
        let slot = after
            .get_mut(protocol::BANK_AMP, 3)
            .expect("the slot exists");
        slot.set_name("New name");
        slot.block_mut(Kind::Amp)
            .expect("the amp block exists")
            .set_byte(32, 0x40);
        let error = verify_rename(&after, before.as_ref(), protocol::BANK_AMP, 3, "New name")
            .expect_err("the payload must not move");
        assert!(error.contains("offset 32"), "{error}");
    }

    #[test]
    fn a_rename_that_the_amp_ignored_is_an_error() {
        let dump = dump_with_slot(0x63);
        let before = dump.get(protocol::BANK_AMP, 3).cloned();
        let error = verify_rename(&dump, before.as_ref(), protocol::BANK_AMP, 3, "New name")
            .expect_err("the old name is still there");
        assert!(error.contains("the name reads \"My preset\""), "{error}");
    }

    #[test]
    fn a_rename_without_an_earlier_copy_still_confirms_the_name() {
        let dump = dump_with_slot(0x63);
        let message = verify_rename(&dump, None, protocol::BANK_AMP, 3, "My preset")
            .expect("the name matches");
        assert!(message.contains("name read back"), "{message}");
    }
}
