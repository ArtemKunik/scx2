//! Background thread that owns the HID device and keeps the app in sync.
//!
//! It never blocks on a dump: a dump takes about two seconds, and knob pushes
//! and writes have to keep flowing while one is running. One loop therefore
//! sends what is queued, handles one report, and starts a dump when one is due.

use std::collections::BTreeMap;
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
    Warning(String),
    Error(String),
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
                }
            }
        }

        let busy = awaiting_ack.is_some()
            || !params_out.is_empty()
            || !blocks_out.is_empty()
            || !bypasses_out.is_empty();
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
            && bypasses_out.is_empty();
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
