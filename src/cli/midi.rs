//! Headless MIDI foot-controller bridge.
//!
//! The editor already does this while its window is open. This command is for
//! a laptop or a small box left connected to the amp: no GUI, one MIDI input,
//! the same learned bindings from `midi.json`.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use scx2::midi::{self, Action, Effect, Router};
use scx2::protocol::{self, Dump, Message, BANK_LIVE, OFF_BYPASS};
use scx2::transport::Amp;

/// Minimum gap between bypass frames, so a controller cannot outrun the amp.
const WRITE_GAP: Duration = Duration::from_millis(25);

pub(crate) fn run(
    port: Option<&str>,
    config_file: Option<&Path>,
    list: bool,
    path: Option<&str>,
    yes: bool,
) -> Result<()> {
    if list {
        for name in midi::ports().context("listing MIDI inputs")? {
            println!("{name}");
        }
        return Ok(());
    }

    let mut config = match config_file {
        Some(file) => midi::Config::load_from(file)?,
        None => {
            let (config, error) = midi::Config::load();
            if let Some(error) = error {
                bail!("{error}");
            }
            config
        }
    };
    if let Some(port) = port {
        config.port = Some(port.to_string());
        config.enabled = true;
    }
    let Some(port) = config.port.clone() else {
        bail!("no MIDI input selected: pass --port, or save one in the editor's MIDI dialog");
    };
    if config.bindings.is_empty() {
        bail!("no MIDI controls are mapped: learn them in the editor's MIDI dialog first");
    }

    println!("MIDI input: {port}");
    for (effect, binding) in &config.bindings {
        println!(
            "  {:<7} <- {} ({})",
            effect.label(),
            binding.trigger.label(),
            binding.mode.label()
        );
    }

    let amp = Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    if let Some(build) = amp.handshake()? {
        println!("firmware build: {build}");
    }

    // Start from the amp's current live buffer, so a latching control does not
    // flip an effect that is already in the state it asks for.
    let dump = amp.read_dump()?;
    let mut states = states_from(&dump);
    let mut slots = slots_from(&dump);

    let (tx, rx) = std::sync::mpsc::channel();
    let _listener = midi::connect(&port, move |event| {
        let _ = tx.send(event);
    })?;
    println!(
        "listening (ctrl-c to stop){}",
        if yes {
            ""
        } else {
            "; dry run, pass --yes to write"
        }
    );

    let router = Router::new(config);
    let mut last_write = Instant::now() - WRITE_GAP;
    // A write makes the amp re-stream its blocks. Ignore that echo for the
    // effect we just changed; an older value must not pull the bridge back.
    let mut ignore: BTreeMap<Effect, Instant> = BTreeMap::new();

    loop {
        while let Ok(event) = rx.try_recv() {
            let Some(action) = router.handle(event, &states) else {
                continue;
            };
            let Some(slot) = slots.get(&action.effect).copied() else {
                println!(
                    "{}: nothing loaded in the live buffer, ignored",
                    action.effect.label()
                );
                continue;
            };
            send_action(&amp, action, slot, yes, &mut last_write)?;
            states.insert(action.effect, action.bypassed);
            ignore.insert(action.effect, Instant::now() + Duration::from_secs(1));
        }

        // The amp streams live blocks after a front-panel change. Track those
        // instead of dumping all 34 presets while someone is playing.
        if let Some(report) = amp.read_report(50)? {
            if let Some(message) = Message::parse(&report) {
                if message.bank() == BANK_LIVE {
                    let effect = midi::EFFECTS
                        .iter()
                        .copied()
                        .find(|effect| effect.kind() == message.kind());
                    let quiet = effect
                        .and_then(|effect| ignore.get(&effect).copied())
                        .is_some_and(|until| Instant::now() < until);
                    if !quiet {
                        note_block(&message, &mut states, &mut slots);
                    }
                }
            }
        }
    }
}

fn send_action(
    amp: &Amp,
    action: Action,
    slot: u8,
    yes: bool,
    last_write: &mut Instant,
) -> Result<()> {
    let frame = protocol::bypass(action.effect.kind(), slot, action.bypassed);
    let state = if action.bypassed { "off" } else { "on" };
    if !yes {
        println!(
            "{} {state} (slot {slot}) — dry run: {}",
            action.effect.label(),
            protocol::hex(&frame)
        );
        return Ok(());
    }

    let waited = last_write.elapsed();
    if waited < WRITE_GAP {
        std::thread::sleep(WRITE_GAP - waited);
    }
    amp.write_report(&frame)?;
    *last_write = Instant::now();
    println!("{} {state} (slot {slot}) sent", action.effect.label());
    Ok(())
}

fn states_from(dump: &Dump) -> BTreeMap<Effect, bool> {
    let mut states = BTreeMap::new();
    if let Some(live) = dump.live() {
        for effect in midi::EFFECTS {
            let bypassed = live
                .block(effect.kind())
                .is_some_and(|block| block.byte(OFF_BYPASS) != 0);
            states.insert(effect, bypassed);
        }
    }
    states
}

fn slots_from(dump: &Dump) -> BTreeMap<Effect, u8> {
    let mut slots = BTreeMap::new();
    if let Some(live) = dump.live() {
        for effect in midi::EFFECTS {
            let Some(block) = live.block(effect.kind()) else {
                continue;
            };
            if block.model().is_some_and(|model| model != 0) {
                if let Some(slot) = block.slot() {
                    slots.insert(effect, slot);
                }
            }
        }
    }
    slots
}

fn note_block(
    message: &Message,
    states: &mut BTreeMap<Effect, bool>,
    slots: &mut BTreeMap<Effect, u8>,
) {
    for effect in midi::EFFECTS {
        if effect.kind() != message.kind() {
            continue;
        }
        states.insert(effect, message.byte(OFF_BYPASS) != 0);
        let loaded = message.model().is_some_and(|model| model != 0);
        match message.slot().filter(|_| loaded) {
            Some(slot) => {
                slots.insert(effect, slot);
            }
            None => {
                slots.remove(&effect);
            }
        }
    }
}
