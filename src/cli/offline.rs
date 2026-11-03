//! Commands that work on files: replay, show, diff, plus the shared
//! backup load/save helpers and the table renderer.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use scx2::capture;
use scx2::models;
use scx2::protocol::{self, Dump, Kind, Preset, BANK_AMP, BANK_FX, BANK_LIVE};
pub(crate) fn replay(pcap: &Path, json: bool, out: Option<&Path>) -> Result<()> {
    let reports = capture::amp_reports(pcap)?;
    let dump = Dump::from_reports(reports);
    if let Some(out) = out {
        write_dump(&dump, out)?;
        println!("saved {}", out.display());
    } else if json {
        println!("{}", serde_json::to_string_pretty(&dump.to_json())?);
    } else {
        print_dump(&dump);
    }
    Ok(())
}

pub(crate) fn write_dump(dump: &Dump, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(&dump.to_json())?;
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub(crate) fn default_dump_name() -> String {
    "scx2-dump.json".to_string()
}

pub(crate) fn load_dump(path: &Path) -> Result<Dump> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Dump::from_json(&text).with_context(|| format!("parsing {}", path.display()))
}

pub(crate) fn show(file: &Path, preset: Option<&str>, raw: bool) -> Result<()> {
    let dump = load_dump(file)?;
    match preset {
        Some(selector) => {
            let (bank, index) = parse_selector(selector)?;
            let preset = dump
                .get(bank, index)
                .with_context(|| format!("no preset {bank}:{index} in {}", file.display()))?;
            print_preset(preset, raw);
        }
        None => print_dump(&dump),
    }
    Ok(())
}

pub(crate) fn parse_selector(text: &str) -> Result<(u8, u8)> {
    let (bank, index) = text
        .split_once(':')
        .with_context(|| format!("expected bank:index, got {text:?}"))?;
    Ok((bank.trim().parse()?, index.trim().parse()?))
}

pub(crate) fn diff(before: &Path, after: &Path) -> Result<()> {
    let a = load_dump(before)?;
    let b = load_dump(after)?;
    let changes = protocol::diff_dumps(&a, &b);
    if changes.is_empty() {
        println!("no differences");
        return Ok(());
    }
    println!("{} byte(s) differ\n", changes.len());
    for change in &changes {
        println!("{}", change.line());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// rendering
// ---------------------------------------------------------------------------

pub(crate) fn print_dump(dump: &Dump) {
    let presets: Vec<&Preset> = dump.presets().collect();
    println!(
        "{} preset(s){}",
        presets.len(),
        if dump.ended { "" } else { ", dump incomplete" }
    );
    println!(
        "{:<5} {:>3}  {:<26} {:<17} {:<17} {:<15} {:<13} {:<13}",
        "bank", "idx", "name", "amp", "stomp", "mod", "delay", "reverb"
    );
    println!("{}", "-".repeat(114));
    for preset in presets {
        let cells = [
            (Kind::Amp, 17usize),
            (Kind::Stomp, 17),
            (Kind::Mod, 15),
            (Kind::Delay, 13),
            (Kind::Reverb, 13),
        ]
        .map(|(kind, width)| cell(preset, kind, width));
        println!(
            "{:<5} {:>3}  {:<26} {} {} {} {} {}",
            bank_label(preset.bank),
            preset.index,
            truncate(&preset.name, 26),
            cells[0],
            cells[1],
            cells[2],
            cells[3],
            cells[4]
        );
    }
    if !dump.unparsed.is_empty() {
        println!("\n{} report(s) not decoded:", dump.unparsed.len());
        for report in &dump.unparsed {
            println!("  {}", protocol::hex(report));
        }
    }
}

pub(crate) fn print_preset(preset: &Preset, raw: bool) {
    println!(
        "[{} {}] {}",
        bank_label(preset.bank),
        preset.index,
        if preset.name.is_empty() {
            "(no name)"
        } else {
            &preset.name
        }
    );
    for message in preset.messages() {
        let mut line = format!("  {:<7} ", message.kind().label());
        if let Some(model) = message.model() {
            let name = models::model_name(message.kind(), model).unwrap_or("unknown");
            line.push_str(&format!("0x{model:04x}  {name}"));
            if let Some(slot) = message.slot() {
                line.push_str(&format!("  slot {slot}"));
            }
        }
        println!("{line}");
        if raw {
            println!("          {}", protocol::hex(message.as_bytes()));
        }
    }
    if let Some(amp) = preset.block(Kind::Amp) {
        println!("  params  {}", protocol::hex(amp.params()));
    }
}

pub(crate) fn cell(preset: &Preset, kind: Kind, width: usize) -> String {
    let text = preset
        .block(kind)
        .and_then(|message| message.model())
        .map(|model| match models::model_name(kind, model) {
            Some(name) => name.to_string(),
            None => format!("0x{model:04x}"),
        })
        .unwrap_or_else(|| "-".to_string());
    format!("{:<width$}", truncate(&text, width), width = width)
}

pub(crate) fn bank_label(bank: u8) -> &'static str {
    match bank {
        BANK_LIVE => "live",
        BANK_AMP => "amp",
        BANK_FX => "fx",
        _ => "?",
    }
}

pub(crate) fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "Ã¢â‚¬Â¦"
}
