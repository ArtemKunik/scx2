//! Direct-control commands: param, probe and their parsers.

use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use scx2::protocol::{self, Kind};
use scx2::{params, transport};

/// Send one direct parameter change and show what the amp reports afterwards.
pub(crate) fn param(
    index: u8,
    value: u8,
    selector: u8,
    model: Option<&str>,
    path: Option<&str>,
    yes: bool,
) -> Result<()> {
    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    amp.handshake()?;
    let before = amp.read_dump()?;
    let live = before.live().context("the dump has no live buffer")?;
    let model_id = match model {
        Some(text) => parse_model(text)?,
        None => live
            .block(Kind::Amp)
            .and_then(|block| block.model())
            .context("the live buffer has no amp block")?,
    };
    let magic_value = params::magic(value);
    let frame = params::direct_control(model_id, index, selector, magic_value);

    println!(
        "param {index} ({}) value {value} -> magic 0x{magic_value:04x}",
        params::label(index)
    );
    println!("frame: {}", protocol::hex(&frame));
    if !yes {
        println!("dry run, add --yes to transmit");
        return Ok(());
    }

    amp.write_report(&frame)?;
    sleep(Duration::from_millis(150));
    let after = amp.read_dump()?;
    let Some(after_live) = after.live() else {
        println!("the amp did not report a live buffer after the change");
        return Ok(());
    };
    let changes = protocol::diff_presets(live, after_live);
    if changes.is_empty() {
        println!("the amp accepted the frame but reported no change");
    } else {
        for change in &changes {
            println!("{}", change.line());
        }
    }
    Ok(())
}

/// Sweep parameter indices, report the offsets each one moves, then restore.
pub(crate) fn probe(
    from: u8,
    to: u8,
    value: u8,
    selector: u8,
    path: Option<&str>,
    yes: bool,
) -> Result<()> {
    let indices: Vec<u8> = (from..=to).collect();
    if !yes {
        println!("probe would send {} frame(s):", indices.len());
        for index in indices {
            let frame = params::direct_control(0, index, selector, params::magic(value));
            println!(
                "  {index:02x} {:<20} {}",
                params::label(index),
                protocol::hex(&frame)
            );
        }
        println!("dry run, add --yes to transmit");
        return Ok(());
    }

    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    amp.handshake()?;
    let baseline = amp.read_dump()?;
    let Some(live) = baseline.live() else {
        bail!("the dump has no live buffer");
    };
    let model = live
        .block(Kind::Amp)
        .and_then(|block| block.model())
        .context("the live buffer has no amp block")?;
    let original = live
        .block(Kind::Amp)
        .map(|block| *block.as_bytes())
        .context("the live buffer has no amp block")?;

    println!("model 0x{model:04x}, probing value {value}\n");
    let mut previous = baseline.clone();
    for index in indices {
        let frame = params::direct_control(model, index, selector, params::magic(value));
        amp.write_report(&frame)?;
        sleep(Duration::from_millis(150));
        let current = amp.read_dump()?;
        let mut lines = Vec::new();
        if let (Some(before), Some(after)) = (previous.live(), current.live()) {
            for change in protocol::diff_presets(before, after) {
                if change.offset < protocol::OFF_PARAMS {
                    continue;
                }
                lines.push(format!(
                    "offset {} 0x{:02x} -> 0x{:02x}",
                    change.offset, change.before, change.after
                ));
            }
        }
        if lines.is_empty() {
            println!("{index:02x} {:<20} no change", params::label(index));
        } else {
            println!(
                "{index:02x} {:<20} {}",
                params::label(index),
                lines.join(", ")
            );
        }
        previous = current;
    }

    // Put the amp back exactly as it was.
    amp.write_report(&protocol::as_host_frame(&original, protocol::HOST_CONTROL))?;
    amp.write_report(&protocol::execute_model_change())?;
    sleep(Duration::from_millis(150));
    let restored = amp.read_dump()?;
    let dirty: Vec<_> = match restored.live() {
        Some(live_after) => protocol::diff_presets(live, live_after)
            .into_iter()
            .filter(|change| change.offset != 7)
            .collect(),
        None => Vec::new(),
    };
    if dirty.is_empty() {
        println!("\nrestored the original settings");
    } else {
        println!(
            "\nwarning: {} byte(s) still differ after restore",
            dirty.len()
        );
        for change in dirty {
            println!("  {}", change.line());
        }
    }
    Ok(())
}

pub(crate) fn parse_model(text: &str) -> Result<u16> {
    let text = text.trim();
    let parsed = if let Some(hex) = text.strip_prefix("0x") {
        u16::from_str_radix(hex, 16)
    } else {
        text.parse::<u16>()
    };
    parsed.with_context(|| format!("model must be like 0x53 or 83, got {text:?}"))
}
