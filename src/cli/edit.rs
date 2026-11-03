//! Commands that write to the amp: send, apply, restore.

use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use scx2::protocol::{self, Kind, Preset};
use scx2::{params, transport};

use super::offline::load_dump;

/// Parse a value given as decimal or `0x` hex.
pub(crate) fn parse_value(text: &str) -> Result<u8> {
    let text = text.trim();
    let parsed = if let Some(hex) = text.strip_prefix("0x") {
        u8::from_str_radix(hex, 16)
    } else {
        text.parse::<u8>()
    };
    parsed.with_context(|| format!("value must be 0..255 or 0x00..0xff, got {text:?}"))
}
pub(crate) fn send(hex: &str, path: Option<&str>, yes: bool, execute: bool) -> Result<()> {
    let bytes = protocol::parse_hex(hex).with_context(|| format!("not valid hex: {hex:?}"))?;
    if bytes.len() > protocol::REPORT_SIZE {
        bail!("report is {} bytes, max 64", bytes.len());
    }
    let mut frame = [0u8; protocol::REPORT_SIZE];
    frame[..bytes.len()].copy_from_slice(&bytes);

    println!("frame: {}", protocol::hex(&frame));
    if !yes {
        println!("dry run, add --yes to transmit");
        return Ok(());
    }
    let amp = transport::Amp::open(path)?;
    amp.handshake()?;
    amp.write_report(&frame)?;
    println!("sent");
    if execute {
        let exec = protocol::execute_model_change();
        amp.write_report(&exec)?;
        println!("sent execute frame: {}", protocol::hex(&exec));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    kind: &str,
    offset: usize,
    value: &str,
    bank: u8,
    index: u8,
    dir: &str,
    from: Option<&Path>,
    path: Option<&str>,
    yes: bool,
    execute: bool,
) -> Result<()> {
    let kind = Kind::parse(kind).with_context(|| format!("unknown block {kind:?}"))?;
    let value = parse_value(value)?;
    if offset >= protocol::REPORT_SIZE {
        bail!("offset must be 0..{}", protocol::REPORT_SIZE - 1);
    }
    let direction = match dir {
        "c3" => protocol::HOST_CONTROL,
        "c1" => protocol::HOST_REQUEST,
        "echo" => protocol::AMP_TO_HOST,
        other => bail!("--dir must be c3, c1 or echo, got {other:?}"),
    };

    // Read the current block so the frame keeps every other byte: from a
    // backup file when one is given, otherwise from the amp.
    let (source, mut amp) = match from {
        Some(file) => (load_dump(file)?, None),
        None => {
            let amp = transport::Amp::open(path)?;
            println!("connected: {}", amp.describe()?);
            amp.handshake()?;
            let dump = amp.read_dump()?;
            (dump, Some(amp))
        }
    };
    let preset = source
        .get(bank, index)
        .with_context(|| format!("no preset {bank}:{index} in the source"))?;
    let block = preset
        .block(kind)
        .with_context(|| format!("preset {bank}:{index} has no {} block", kind.label()))?;
    let mut raw = *block.as_bytes();
    let before = raw[offset];
    raw[offset] = value;

    // Parameters the amp exposes through direct control ignore block writes,
    // so address them the way the amp's own knobs do.
    let direct = (kind == Kind::Amp)
        .then(|| params::index_for_offset(offset))
        .flatten()
        .map(|param| {
            let model = block.model().unwrap_or(0);
            params::direct_control(
                model,
                param,
                params::SELECTOR_AMP,
                params::magic_from_byte(value),
            )
        });
    let frame = direct.unwrap_or_else(|| protocol::as_host_frame(&raw, direction));
    let needs_execute = direct.is_none() && execute;

    println!(
        "{} {}.{} offset {}: 0x{:02x} -> 0x{:02x}",
        kind.label(),
        bank,
        index,
        offset,
        before,
        value
    );
    println!("frame: {}", protocol::hex(&frame));
    if !yes {
        println!("dry run, add --yes to transmit");
        return Ok(());
    }
    let amp = match amp.take() {
        Some(amp) => amp,
        None => {
            let amp = transport::Amp::open(path)?;
            amp.handshake()?;
            amp
        }
    };
    amp.write_report(&frame)?;
    println!("sent");
    if needs_execute {
        let exec = protocol::execute_model_change();
        amp.write_report(&exec)?;
        println!("sent execute frame: {}", protocol::hex(&exec));
    }
    Ok(())
}

/// Recall a stored preset.
///
/// The frame is the Mustang patch-change shape. It is deliberately marked
/// experimental: the SCX2 has not confirmed it, and the bank pairing is
/// unknown. The command still refuses to send without `--yes`.
pub(crate) fn recall(bank: u8, slot: u8, path: Option<&str>, yes: bool) -> Result<()> {
    let frame = protocol::recall_preset(bank, slot);
    println!("recall bank {bank} slot {slot} (experimental, unverified on SCX2)");
    println!("frame: {}", protocol::hex(&frame));
    if !yes {
        println!("dry run, add --yes to transmit");
        return Ok(());
    }
    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    amp.handshake()?;
    amp.write_report(&frame)?;
    println!("sent");
    Ok(())
}

/// Rename a stored preset by writing its `0x04` name block.
///
/// Writing the name block is not confirmed to persist a stored slot; the
/// command prints the exact frame and only sends with `--yes`.
pub(crate) fn rename(name: &str, bank: u8, index: u8, path: Option<&str>, yes: bool) -> Result<()> {
    let raw = protocol::name_change(bank, index, name);
    let frame = protocol::as_host_frame(&raw, protocol::HOST_CONTROL);
    println!(
        "rename bank {bank} index {index} to {:?} (experimental, unverified on SCX2)",
        name.chars().take(protocol::NAME_LEN).collect::<String>()
    );
    println!("frame: {}", protocol::hex(&frame));
    if !yes {
        println!("dry run, add --yes to transmit");
        return Ok(());
    }
    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    amp.handshake()?;
    amp.write_report(&frame)?;
    amp.write_report(&protocol::execute_model_change())?;
    println!("sent name block + execute frame");
    Ok(())
}

/// Store the live buffer into a preset slot.
///
/// The real save frame is still unknown. This is the best available
/// experiment: every live block is re-addressed to the target bank/index and
/// written with the execute frame the amp already accepts. It prints exactly
/// what it will send and requires `--yes`.
pub(crate) fn store(
    bank: u8,
    index: u8,
    name: Option<&str>,
    path: Option<&str>,
    yes: bool,
) -> Result<()> {
    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    amp.handshake()?;
    let dump = amp.read_dump()?;
    let live = dump
        .live()
        .context("the dump did not contain a live buffer")?;

    let mut frames: Vec<(String, [u8; protocol::REPORT_SIZE])> = Vec::new();
    for message in live.messages() {
        let raw = protocol::retarget(message.as_bytes(), bank, index);
        frames.push((
            message.summary(),
            protocol::as_host_frame(&raw, protocol::HOST_CONTROL),
        ));
    }
    if let Some(name) = name {
        let raw = protocol::name_change(bank, index, name);
        frames.push((
            format!("name={name:?}"),
            protocol::as_host_frame(&raw, protocol::HOST_CONTROL),
        ));
    }

    println!(
        "store live buffer into bank {bank} index {index}: {} block(s)",
        frames.len()
    );
    println!(
        "EXPERIMENTAL: the SCX2 save frame is unknown. This writes each block with the target bank/index and the execute frame. Verify with `scx2 dump` afterwards."
    );
    for (summary, frame) in &frames {
        println!("  {summary}  {}", protocol::hex(frame));
    }
    if !yes {
        println!("dry run, add --yes to transmit");
        return Ok(());
    }
    for (summary, frame) in &frames {
        println!("sending {summary}");
        amp.write_report(frame)?;
        sleep(Duration::from_millis(20));
        amp.write_report(&protocol::execute_model_change())?;
        sleep(Duration::from_millis(20));
    }
    println!("done: {} block(s) sent", frames.len());
    Ok(())
}

pub(crate) fn restore(
    file: &Path,
    path: Option<&str>,
    bank: Option<u8>,
    yes: bool,
    execute: bool,
) -> Result<()> {
    let dump = load_dump(file)?;
    let presets: Vec<&Preset> = match bank {
        Some(bank) => dump.bank(bank).collect(),
        None => dump.presets().collect(),
    };
    if presets.is_empty() {
        bail!("nothing to restore in {}", file.display());
    }

    let total: usize = presets.iter().map(|p| p.messages().len()).sum();
    println!(
        "restoring {} preset(s), {} report(s) from {}",
        presets.len(),
        total,
        file.display()
    );

    let amp = if yes {
        let amp = transport::Amp::open(path)?;
        println!("connected: {}", amp.describe()?);
        amp.handshake()?;
        Some(amp)
    } else {
        None
    };

    for preset in presets {
        for message in preset.messages() {
            let frame = protocol::as_host_frame(message.as_bytes(), protocol::HOST_CONTROL);
            if let Some(amp) = &amp {
                amp.write_report(&frame)?;
                sleep(Duration::from_millis(20));
                if execute {
                    amp.write_report(&protocol::execute_model_change())?;
                    sleep(Duration::from_millis(20));
                }
            } else {
                println!("  {} {}", message.summary(), protocol::hex(&frame));
            }
        }
    }

    if yes {
        println!("done: {total} report(s) sent");
    } else {
        println!("dry run, add --yes to transmit");
    }
    Ok(())
}
