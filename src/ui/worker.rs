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
///
/// A full dump is 195 reports and takes about two seconds, and the amp already
/// announces its own state changes and knob moves. Polling it every few
/// seconds made the amp read almost continuously while someone was playing;
/// this is only a safety net for a missed announcement.
const RESYNC: Duration = Duration::from_secs(30);
/// Do not start a routine dump while someone is turning a knob.
const QUIET: Duration = Duration::from_secs(1);
/// Give up waiting for a write's acknowledgement after this long.
const ACK_WAIT: Duration = Duration::from_millis(150);
const BLOCK_GAP: Duration = Duration::from_millis(60);
const SOON: Duration = Duration::from_millis(150);
const DUMP_LIMIT: Duration = Duration::from_secs(10);
/// Read the live buffer once more after a write, beyond the local grace.
///
/// A fast bypass frame or a block write has no read-back of its own. The first
/// dump 400 ms later confirms an accepted write; this second one lets the UI
/// retire a value the amp never took instead of keeping it on screen until the
/// routine resync.
const RECHECK: Duration = Duration::from_secs(6);
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
    /// Write the live misc block's USB gain and read it back.
    UsbGain {
        value: u8,
        frame: [u8; protocol::REPORT_SIZE],
    },
    /// Write stored presets from a backup, one report at a time.
    Restore { presets: Vec<Preset> },
    /// Stop a running restore after the block already sent.
    CancelRestore,
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
    /// The USB gain was written; `outcome` carries the read-back result.
    UsbGainDone {
        outcome: Result<String, String>,
    },
    /// Block `done` of `total` was written to the amp.
    RestoreProgress {
        done: usize,
        total: usize,
    },
    /// The restore stopped, and the re-dump has been compared with the backup.
    RestoreDone(RestoreReport),
    Warning(String),
    Error(String),
}

/// A device write queued for the worker, in request order.
enum DeviceWrite {
    Store {
        bank: u8,
        index: u8,
        name: String,
    },
    Rename {
        bank: u8,
        index: u8,
        name: String,
    },
    UsbGain {
        value: u8,
        frame: [u8; protocol::REPORT_SIZE],
    },
}

/// A restore in progress: every report to write, in backup order.
struct RestoreJob {
    /// Slot key and frame, in the order the backup lists them.
    reports: VecDeque<(u8, u8, [u8; protocol::REPORT_SIZE])>,
    total: usize,
    written: usize,
    /// Slots whose last report has been sent; only these can be compared.
    done: Vec<(u8, u8)>,
    /// The slot the last sent report belonged to.
    current: Option<(u8, u8)>,
    /// The slots the backup expects, for the read-back comparison.
    source: Vec<Preset>,
}

impl RestoreJob {
    fn new(presets: Vec<Preset>) -> Self {
        let mut reports = VecDeque::new();
        for preset in &presets {
            for message in preset.messages() {
                reports.push_back((
                    preset.bank,
                    preset.index,
                    protocol::as_host_frame(message.as_bytes(), protocol::HOST_CONTROL),
                ));
            }
        }
        RestoreJob {
            total: reports.len(),
            reports,
            written: 0,
            done: Vec::new(),
            current: None,
            source: presets,
        }
    }
}

/// What a finished or cancelled restore wrote, and how it read back.
pub(crate) struct RestoreReport {
    pub written: usize,
    pub total: usize,
    pub slots: usize,
    pub cancelled: bool,
    /// One line per slot that does not match the backup.
    pub mismatches: Vec<String>,
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
        before: Option<Box<Preset>>,
    },
    UsbGain {
        value: u8,
        /// The live misc block before the write, for the byte comparison.
        before: Option<[u8; protocol::REPORT_SIZE]>,
    },
    Restore {
        source: Box<[Preset]>,
        /// A slot the restore started but did not finish, if any.
        partial: Option<(u8, u8)>,
        written: usize,
        total: usize,
        cancelled: bool,
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
            if let Ok(description) = amp.describe() {
                eprintln!("amp connected: {description}");
            }
            send(Ev::Connected(banner));
            run(&amp, &send, &rx)
        });
        if let Err(error) = outcome {
            eprintln!("amp connection failed or closed: {error:#}");
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
    // A backup being written to the amp, one report per loop turn.
    let mut restore: Option<RestoreJob> = None;
    let mut cancel_restore = false;
    let mut awaiting_ack: Option<Instant> = None;
    let mut last_block = Instant::now() - BLOCK_GAP;
    let mut last_activity = Instant::now() - QUIET;
    // Our own block/bypass write can make the amp announce a state change.
    // Do not answer that with a dump; the live buffer on screen is already
    // newer than the copy the amp would send back.
    let mut quiet_until = Instant::now() - QUIET;
    let mut collecting: Option<Collecting> = None;
    let mut dump_soon = Some(Instant::now());
    let mut recheck_at: Option<Instant> = None;
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
                Ok(Cmd::UsbGain { value, frame }) => {
                    device_out.push_back(DeviceWrite::UsbGain { value, frame });
                }
                Ok(Cmd::Restore { presets }) => {
                    restore = Some(RestoreJob::new(presets));
                    cancel_restore = false;
                }
                Ok(Cmd::CancelRestore) => cancel_restore = true,
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
        // A cancel stops between blocks, and still reads the amp back so the
        // person can see which slots were touched.
        if cancel_restore {
            cancel_restore = false;
            if let Some(job) = restore.take() {
                finish_restore(job, true, &mut verify, &mut dump_soon, now);
            }
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
                    // Read the block back promptly; the fast frame has no ack
                    // we can compare against on its own.
                    dump_soon = Some(now + Duration::from_millis(400));
                    recheck_at = Some(now + RECHECK);
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
                    // Do not wait for the 30 s fallback to learn whether the
                    // amp kept the block; read it back once it has applied it.
                    dump_soon = Some(now + Duration::from_millis(400));
                    recheck_at = Some(now + RECHECK);
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
                        DeviceWrite::UsbGain { value, frame } => {
                            amp.write_report(frame)?;
                            amp.write_report(&protocol::execute_model_change())?;
                            let before = last_dump
                                .as_ref()
                                .and_then(|dump| dump.live_misc())
                                .map(|misc| *misc.as_bytes());
                            verify = Some(Verify::UsbGain {
                                value: *value,
                                before,
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
                                before: before.map(Box::new),
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
                } else if let Some(job) = restore.as_mut() {
                    // Restores write stored memory block by block. They wait
                    // for any dump in flight: the two streams would interleave.
                    if collecting.is_none() {
                        if let Some((bank, index, frame)) = job.reports.pop_front() {
                            amp.write_report(&frame)?;
                            amp.write_report(&protocol::execute_model_change())?;
                            job.written += 1;
                            job.current = Some((bank, index));
                            // The last report of a slot is what finishes it.
                            if job.reports.front().map(|next| (next.0, next.1))
                                != Some((bank, index))
                            {
                                job.done.push((bank, index));
                            }
                            last_block = now;
                            last_activity = now;
                            quiet_until = now + Duration::from_secs(1);
                            send(Ev::RestoreProgress {
                                done: job.written,
                                total: job.total,
                            });
                            if job.reports.is_empty() {
                                let job = restore.take().expect("the restore is running");
                                finish_restore(job, false, &mut verify, &mut dump_soon, now);
                            }
                        }
                    }
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
                                        before.as_deref(),
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
                                Some(Verify::UsbGain { value, before }) => {
                                    let outcome =
                                        verify_usb_gain(&done.dump, before.as_ref(), value);
                                    send(Ev::UsbGainDone { outcome });
                                }
                                Some(Verify::Restore {
                                    source,
                                    partial,
                                    written,
                                    total,
                                    cancelled,
                                }) => {
                                    let slots = source.len();
                                    let mut mismatches = verify_restore(&done.dump, &source);
                                    if let Some((bank, index)) = partial {
                                        mismatches.push(format!(
                                            "{} {}: partially written (the restore stopped between its blocks)",
                                            protocol::bank_label(bank),
                                            index
                                        ));
                                    }
                                    send(Ev::RestoreDone(RestoreReport {
                                        written,
                                        total,
                                        slots,
                                        cancelled,
                                        mismatches,
                                    }));
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
            && device_out.is_empty()
            && restore.is_none();
        let recheck = recheck_at.is_some_and(|at| now >= at);
        let asked = dump_soon.is_some_and(|at| now >= at);
        let routine = now >= next_resync && now - last_activity >= QUIET;
        if collecting.is_none() && idle && (asked || recheck || routine) {
            amp.write_report(&protocol::request_dump())?;
            dump_soon = None;
            if recheck {
                recheck_at = None;
            }
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

/// Hand a finished or cancelled restore to the read-back stage.
///
/// Only slots whose every block went out can be compared with the backup:
/// untouched slots still hold their old sound, and the slot in flight is
/// partial by definition.
fn finish_restore(
    job: RestoreJob,
    cancelled: bool,
    verify: &mut Option<Verify>,
    dump_soon: &mut Option<Instant>,
    now: Instant,
) {
    let partial = if cancelled {
        job.current.filter(|key| !job.done.contains(key))
    } else {
        None
    };
    let source: Vec<Preset> = job
        .source
        .into_iter()
        .filter(|preset| job.done.contains(&(preset.bank, preset.index)))
        .collect();
    *verify = Some(Verify::Restore {
        source: source.into_boxed_slice(),
        partial,
        written: job.written,
        total: job.total,
        cancelled,
    });
    *dump_soon = Some(now + Duration::from_millis(400));
}

/// Compare the live misc block with the USB gain that was written.
///
/// The gain sits at offset 16 of the `0x0D` misc report. The header (0..8) may
/// move because the execute frame flips byte 7, but every other payload byte
/// must stay where it was.
fn verify_usb_gain(
    dump: &Dump,
    before: Option<&[u8; protocol::REPORT_SIZE]>,
    value: u8,
) -> Result<String, String> {
    let Some(misc) = dump.live_misc() else {
        return Err("The amp did not report its misc block after the write.".into());
    };
    let read = misc.byte(protocol::USB_GAIN_OFFSET);
    if read != value {
        return Err(format!(
            "USB gain reads {read} ({}%), not {value} ({}%).",
            percent(read),
            percent(value)
        ));
    }
    if let Some(before) = before {
        for (offset, expected) in before.iter().enumerate().skip(protocol::PAYLOAD_START) {
            if offset == protocol::USB_GAIN_OFFSET {
                continue;
            }
            if misc.byte(offset) != *expected {
                return Err(format!(
                    "USB gain was written, but misc offset {offset} changed (0x{expected:02x} -> 0x{:02x}).",
                    misc.byte(offset)
                ));
            }
        }
    }
    Ok(format!(
        "USB gain {value} ({}%) · read-back matches",
        percent(value)
    ))
}

/// 0..=255 as the 0..100% FUSE labels.
pub(crate) fn percent(value: u8) -> u32 {
    (u32::from(value) * 100 + 127) / 255
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
    // A bank stores only its own blocks: the amp/stomp of bank 1, the effects
    // of bank 2. The live buffer has all of them, so comparing the rest would
    // report a mismatch for every effect a voice save legitimately drops.
    let kinds = protocol::bank_kinds(bank);
    let changes: Vec<_> = protocol::diff_payload(live, stored)
        .into_iter()
        .filter(|change| kinds.contains(&change.kind))
        .collect();
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
    for (kind, model) in protocol::models_missing_from(live, stored)
        .into_iter()
        .filter(|(kind, _)| kinds.contains(kind))
    {
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

/// Compare every restored slot with the backup it came from.
///
/// A restore copies whole blocks, so the payload must match; the frame header
/// and the execute flag are excluded. One line per slot that does not match,
/// so a cancelled restore can still report the slots it touched.
fn verify_restore(dump: &Dump, source: &[Preset]) -> Vec<String> {
    let mut mismatches = Vec::new();
    for expected in source {
        let label = format!("{} {}", protocol::bank_label(expected.bank), expected.index);
        let Some(actual) = dump.get(expected.bank, expected.index) else {
            mismatches.push(format!("{label}: the amp does not report this slot"));
            continue;
        };
        let mut problems = Vec::new();
        if actual.name != expected.name {
            problems.push(format!("the name reads {:?}", actual.name));
        }
        let changes = protocol::diff_payload(expected, actual);
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
        for (kind, model) in protocol::models_missing_from(expected, actual) {
            problems.push(format!(
                "the {} model 0x{model:04x} is missing",
                kind.label()
            ));
        }
        if !problems.is_empty() {
            mismatches.push(format!("{label}: {}", problems.join("; ")));
        }
    }
    mismatches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Message, AMP_TO_HOST, FRAME_MARKER, OFF_MODEL, REPORT_SIZE};

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

    fn preset(bank: u8, index: u8, name: &str, volume: u8) -> Preset {
        let mut preset = Preset::new(bank, index);
        preset.add(
            Message::from_bytes(protocol::name_change(bank, index, name))
                .expect("a valid name block"),
        );
        preset.add(
            Message::from_bytes(report(Kind::Amp, bank, index, 0x21, volume))
                .expect("a valid amp block"),
        );
        preset
    }

    fn dump_from(presets: &[Preset]) -> Dump {
        let mut dump = Dump::new();
        for preset in presets {
            for message in preset.messages() {
                dump.add_raw(message.as_bytes());
            }
        }
        dump
    }

    #[test]
    fn a_restore_job_writes_names_before_blocks_in_backup_order() {
        let presets = vec![
            preset(protocol::BANK_AMP, 1, "one", 0x10),
            preset(protocol::BANK_FX, 2, "two", 0x20),
        ];
        let job = RestoreJob::new(presets);
        assert_eq!(job.total, 4, "two messages per preset");
        assert_eq!(job.written, 0);
        assert_eq!(job.reports[0].2[2], Kind::Name.code());
        assert_eq!(job.reports[0].2[1], protocol::HOST_CONTROL);
        assert_eq!(job.reports[1].2[2], Kind::Amp.code());
        assert_eq!(job.reports[2].2[2], Kind::Name.code());
    }

    /// Run the job one report at a time, the way the worker loop does.
    fn write_reports(job: &mut RestoreJob, count: usize) {
        for _ in 0..count {
            let Some((bank, index, _)) = job.reports.pop_front() else {
                break;
            };
            job.written += 1;
            job.current = Some((bank, index));
            if job.reports.front().map(|next| (next.0, next.1)) != Some((bank, index)) {
                job.done.push((bank, index));
            }
        }
    }

    #[test]
    fn a_cancelled_restore_compares_only_the_slots_that_finished() {
        let mut job = RestoreJob::new(vec![
            preset(protocol::BANK_AMP, 1, "one", 0x10),
            preset(protocol::BANK_FX, 2, "two", 0x20),
        ]);
        // One report: the name of amp 1, nothing complete yet.
        write_reports(&mut job, 1);
        let mut verify = None;
        let mut dump_soon = None;
        finish_restore(job, true, &mut verify, &mut dump_soon, Instant::now());
        match verify {
            Some(Verify::Restore {
                source, partial, ..
            }) => {
                assert!(source.is_empty(), "no slot finished");
                assert_eq!(partial, Some((protocol::BANK_AMP, 1)));
            }
            _ => panic!("a restore verify was expected"),
        }
        assert!(dump_soon.is_some(), "a re-dump is still requested");
    }

    #[test]
    fn a_finished_restore_compares_every_slot_the_backup_lists() {
        let mut job = RestoreJob::new(vec![
            preset(protocol::BANK_AMP, 1, "one", 0x10),
            preset(protocol::BANK_FX, 2, "two", 0x20),
        ]);
        write_reports(&mut job, 4);
        let mut verify = None;
        let mut dump_soon = None;
        finish_restore(job, false, &mut verify, &mut dump_soon, Instant::now());
        match verify {
            Some(Verify::Restore {
                source, partial, ..
            }) => {
                assert_eq!(source.len(), 2);
                assert_eq!(partial, None);
            }
            _ => panic!("a restore verify was expected"),
        }
    }

    #[test]
    fn a_restore_that_matches_the_backup_has_no_mismatches() {
        let source = vec![preset(protocol::BANK_AMP, 3, "My preset", 0x63)];
        let dump = dump_from(&source);
        assert!(verify_restore(&dump, &source).is_empty());
    }

    #[test]
    fn a_slot_that_kept_an_older_value_is_reported() {
        let source = vec![preset(protocol::BANK_AMP, 3, "My preset", 0x63)];
        let mut dump = dump_from(&source);
        dump.get_mut(protocol::BANK_AMP, 3)
            .and_then(|preset| preset.block_mut(Kind::Amp))
            .expect("the amp block exists")
            .set_byte(32, 0x40);
        let mismatches = verify_restore(&dump, &source);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].contains("amp 3"), "{mismatches:?}");
        assert!(mismatches[0].contains("offset 32"), "{mismatches:?}");
    }

    #[test]
    fn a_slot_missing_from_the_re_dump_is_reported() {
        let source = vec![preset(protocol::BANK_FX, 2, "gone", 0x63)];
        let dump = dump_from(&[]);
        let mismatches = verify_restore(&dump, &source);
        assert_eq!(mismatches.len(), 1);
        assert!(mismatches[0].contains("does not report"), "{mismatches:?}");
    }

    /// A live buffer with every block, plus a stored slot carrying only what
    /// its bank keeps.
    fn dump_with_bank_slot(stored_bank: u8) -> Dump {
        let mut dump = Dump::new();
        dump.add_raw(&protocol::name_change(BANK_LIVE, 0, "live"));
        dump.add_raw(&report(Kind::Amp, BANK_LIVE, 0, 0x21, 0x63));
        dump.add_raw(&report(Kind::Stomp, BANK_LIVE, 0, 0x0007, 0x80));
        dump.add_raw(&report(Kind::Mod, BANK_LIVE, 0, 0x0012, 0x81));
        dump.add_raw(&report(Kind::Delay, BANK_LIVE, 0, 0x002b, 0x82));
        dump.add_raw(&report(Kind::Reverb, BANK_LIVE, 0, 0x0024, 0x83));
        dump.add_raw(&protocol::name_change(stored_bank, 4, "Stored"));
        match stored_bank {
            protocol::BANK_AMP => {
                dump.add_raw(&report(Kind::Amp, stored_bank, 4, 0x21, 0x63));
                dump.add_raw(&report(Kind::Stomp, stored_bank, 4, 0x0007, 0x80));
            }
            protocol::BANK_FX => {
                dump.add_raw(&report(Kind::Mod, stored_bank, 4, 0x0012, 0x81));
                dump.add_raw(&report(Kind::Delay, stored_bank, 4, 0x002b, 0x82));
                dump.add_raw(&report(Kind::Reverb, stored_bank, 4, 0x0024, 0x83));
            }
            _ => {}
        }
        dump
    }

    #[test]
    fn a_voice_store_does_not_miss_the_effects_a_voice_slot_never_keeps() {
        let dump = dump_with_bank_slot(protocol::BANK_AMP);
        let outcome = verify_store(&dump, protocol::BANK_AMP, 4, "Stored");
        assert!(outcome.is_ok(), "{outcome:?}");
    }

    #[test]
    fn an_effects_store_does_not_miss_the_amp_block_it_never_keeps() {
        let dump = dump_with_bank_slot(protocol::BANK_FX);
        let outcome = verify_store(&dump, protocol::BANK_FX, 4, "Stored");
        assert!(outcome.is_ok(), "{outcome:?}");
    }

    #[test]
    fn an_effects_store_still_catches_a_changed_reverb() {
        let mut dump = dump_with_bank_slot(protocol::BANK_FX);
        dump.get_mut(protocol::BANK_FX, 4)
            .and_then(|preset| preset.block_mut(Kind::Reverb))
            .expect("the stored reverb exists")
            .set_byte(32, 0x40);
        let error = verify_store(&dump, protocol::BANK_FX, 4, "Stored")
            .expect_err("the reverb kept an older value");
        assert!(error.contains("reverb offset 32"), "{error}");
    }

    fn misc_report(value: u8) -> [u8; REPORT_SIZE] {
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = Kind::Misc.code();
        raw[3] = BANK_LIVE;
        raw[4] = 0x10;
        raw[6] = 1;
        raw[protocol::USB_GAIN_OFFSET] = value;
        raw
    }

    fn dump_with_misc(value: u8) -> Dump {
        let mut dump = Dump::new();
        dump.add_raw(&misc_report(value));
        dump
    }

    #[test]
    fn a_matching_usb_gain_reads_back() {
        let before = misc_report(128);
        let dump = dump_with_misc(128);
        let message = verify_usb_gain(&dump, Some(&before), 128).expect("the gain matches");
        assert!(message.contains("50%"), "{message}");
    }

    #[test]
    fn a_usb_gain_the_amp_ignored_is_an_error() {
        let before = misc_report(64);
        let dump = dump_with_misc(64);
        let error =
            verify_usb_gain(&dump, Some(&before), 128).expect_err("the amp kept the old gain");
        assert!(error.contains("reads 64 (25%)"), "{error}");
    }

    #[test]
    fn another_misc_byte_moving_is_an_error() {
        let mut before = misc_report(128);
        before[9] = 0x01;
        let dump = dump_with_misc(128);
        let error = verify_usb_gain(&dump, Some(&before), 128)
            .expect_err("byte 9 moved without being asked");
        assert!(error.contains("offset 9 changed"), "{error}");
    }

    #[test]
    fn the_execute_flag_flipping_is_not_an_error() {
        let mut before = misc_report(128);
        before[7] = 0x00;
        let mut after = misc_report(128);
        after[7] = 0x01;
        let mut dump = Dump::new();
        dump.add_raw(&after);
        assert!(verify_usb_gain(&dump, Some(&before), 128).is_ok());
    }

    #[test]
    fn a_missing_misc_block_is_an_error() {
        let dump = Dump::new();
        let error = verify_usb_gain(&dump, None, 128).expect_err("there is no misc block to check");
        assert!(error.contains("did not report its misc block"), "{error}");
    }

    #[test]
    fn the_percent_mapping_covers_the_byte_range() {
        assert_eq!(percent(0), 0);
        assert_eq!(percent(255), 100);
        assert_eq!(percent(128), 50);
    }
}
