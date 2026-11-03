//! `scx2` -- command line front end.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use scx2::protocol::{self, Dump, Kind, Preset, BANK_AMP, BANK_FX, BANK_LIVE};
use scx2::{capture, models, transport};

#[derive(Parser)]
#[command(
    name = "scx2",
    version,
    about = "Super Champ X2 preset librarian (unofficial)",
    long_about = "Talking to a Fender Super Champ X2 over USB: back up the preset memory, \
                  watch the amp live, map parameters and (once verified) write them back.\n\n\
                  Everything is read-only unless you pass --yes to a write command."
)]
struct Cli {
    /// Omit to open the desktop editor.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List HID devices (Fender only unless --all)
    Scan {
        /// Show every HID device, not just Fender ones
        #[arg(long)]
        all: bool,
    },
    /// Decode a USBPcap capture offline, no amp needed
    Replay {
        /// Path to a .pcap written by USBPcap
        pcap: PathBuf,
        /// Emit the dump as JSON
        #[arg(long)]
        json: bool,
        /// Write the dump to this file
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Read every preset from the amp into a JSON backup
    Dump {
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Where to write the JSON backup
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Print a backup file
    Show {
        /// JSON file written by `scx2 dump`
        file: PathBuf,
        /// Only this preset, as `bank:index` (e.g. `1:7`)
        #[arg(long)]
        preset: Option<String>,
        /// Print the raw report of every block
        #[arg(long)]
        raw: bool,
    },
    /// Compare two backups byte by byte
    Diff {
        /// Older backup
        before: PathBuf,
        /// Newer backup
        after: PathBuf,
    },
    /// Print reports as the amp sends them
    Monitor {
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Read timeout in milliseconds
        #[arg(long, default_value_t = 1000)]
        timeout: i32,
    },
    /// Poll the live buffer and report which bytes changed
    ///
    /// Turn one knob on the amp between polls; the offsets it prints are the
    /// parameters to map.
    Learn {
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Seconds to wait between dumps
        #[arg(long, default_value_t = 1)]
        interval: u64,
        /// Stop after this many dumps (default: run until ctrl-c)
        #[arg(long)]
        count: Option<usize>,
    },
    /// Send one raw 64-byte report
    Send {
        /// Report as hex, e.g. "1c 01 05 00 00 00 01 01"
        #[arg(long)]
        hex: String,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Actually transmit
        #[arg(long)]
        yes: bool,
        /// Do not send the model-change execute frame (the amp needs it)
        #[arg(long)]
        no_execute: bool,
    },
    /// Change one byte of a block and send it back
    Apply {
        /// Block to patch: amp, stomp, mod, delay, reverb, state
        kind: String,
        /// Report offset, 0..63 (parameters start at 32)
        #[arg(long)]
        offset: usize,
        /// New value, decimal or 0x hex
        #[arg(long)]
        value: String,
        /// Bank that holds the block (0 live, 1 amp, 2 fx)
        #[arg(long, default_value_t = BANK_LIVE)]
        bank: u8,
        /// Preset index inside the bank
        #[arg(long, default_value_t = 0)]
        index: u8,
        /// Direction byte: c3 (default), c1 or echo
        #[arg(long, default_value = "c3")]
        dir: String,
        /// Take the block from a backup instead of reading the amp
        #[arg(long)]
        from: Option<PathBuf>,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Actually transmit
        #[arg(long)]
        yes: bool,
        /// Do not send the model-change execute frame (the amp needs it)
        #[arg(long)]
        no_execute: bool,
    },
    /// Write a backup file back to the amp
    Restore {
        /// JSON file written by `scx2 dump`
        file: PathBuf,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Only restore this bank (0 live, 1 amp, 2 fx)
        #[arg(long)]
        bank: Option<u8>,
        /// Actually transmit
        #[arg(long)]
        yes: bool,
        /// Do not send the model-change execute frame after every block
        #[arg(long)]
        no_execute: bool,
    },
    /// Open the desktop editor
    #[cfg(feature = "ui")]
    Ui {
        /// Backup to open at start (defaults to the bundled demo capture)
        #[arg(long)]
        from: Option<PathBuf>,
        /// HID path to connect to at start
        #[arg(long)]
        path: Option<String>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    fn FreeConsole() -> i32;
}

/// Close the console window when this process is its only user.
#[cfg(windows)]
fn detach_console_if_owner() {
    let mut list = [0u32; 4];
    // SAFETY: the pointer and length describe a stack buffer we own.
    unsafe {
        let attached = GetConsoleProcessList(list.as_mut_ptr(), list.len() as u32);
        if attached == 1 {
            FreeConsole();
        }
    }
}

#[cfg(not(windows))]
fn detach_console_if_owner() {}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: *mut std::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

/// GUI failures would otherwise vanish when the console is detached.
fn report_ui_error(message: &str) {
    #[cfg(windows)]
    {
        let text: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
        let caption: Vec<u16> = "Super Champ X2"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: both pointers reference null-terminated buffers alive here.
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = message;
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let command = match cli.command {
        Some(command) => command,
        #[cfg(feature = "ui")]
        None => {
            // Double-clicking the exe should open the editor without leaving a
            // console window behind. A console that also hosts a shell is left
            // untouched, so `scx2 ui` from a terminal behaves normally.
            detach_console_if_owner();
            Command::Ui {
                from: None,
                path: None,
            }
        }
        #[cfg(not(feature = "ui"))]
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            return Ok(());
        }
    };

    match command {
        Command::Scan { all } => scan(all),
        Command::Replay { pcap, json, out } => replay(&pcap, json, out.as_deref()),
        Command::Dump { path, out } => dump(path.as_deref(), out),
        Command::Show { file, preset, raw } => show(&file, preset.as_deref(), raw),
        Command::Diff { before, after } => diff(&before, &after),
        Command::Monitor { path, timeout } => monitor(path.as_deref(), timeout),
        Command::Learn {
            path,
            interval,
            count,
        } => learn(path.as_deref(), interval, count),
        Command::Send {
            hex,
            path,
            yes,
            no_execute,
        } => send(&hex, path.as_deref(), yes, !no_execute),
        Command::Apply {
            kind,
            offset,
            value,
            bank,
            index,
            dir,
            from,
            path,
            yes,
            no_execute,
        } => apply(
            &kind,
            offset,
            &value,
            bank,
            index,
            &dir,
            from.as_deref(),
            path.as_deref(),
            yes,
            !no_execute,
        ),
        Command::Restore {
            file,
            path,
            bank,
            yes,
            no_execute,
        } => restore(&file, path.as_deref(), bank, yes, !no_execute),
        #[cfg(feature = "ui")]
        Command::Ui { from, path } => {
            if let Err(error) = scx2::ui::run(from, path) {
                report_ui_error(&format!("{error:#}"));
                return Err(error);
            }
            Ok(())
        }
    }
}

fn scan(all: bool) -> Result<()> {
    let devices = transport::scan(!all)?;
    if devices.is_empty() {
        println!("no devices found");
        if !all {
            println!("hint: `scx2 scan --all` lists every HID device");
        }
        return Ok(());
    }
    for device in devices {
        println!("{}", device.label());
        if let Some(manufacturer) = &device.manufacturer {
            println!("    manufacturer: {manufacturer}");
        }
        if let Some(serial) = &device.serial {
            println!("    serial:       {serial}");
        }
    }
    Ok(())
}

fn replay(pcap: &Path, json: bool, out: Option<&Path>) -> Result<()> {
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

fn dump(path: Option<&str>, out: Option<PathBuf>) -> Result<()> {
    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    if let Some(build) = amp.handshake()? {
        println!("firmware build: {build}");
    }
    println!("handshake done, requesting preset dump...");
    let dump = amp.read_dump()?;
    print_dump(&dump);

    let out = out.unwrap_or_else(|| PathBuf::from(default_dump_name()));
    write_dump(&dump, &out)?;
    println!("\nsaved {}", out.display());
    Ok(())
}

fn write_dump(dump: &Dump, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(&dump.to_json())?;
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn default_dump_name() -> String {
    "scx2-dump.json".to_string()
}

fn load_dump(path: &Path) -> Result<Dump> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Dump::from_json(&text).with_context(|| format!("parsing {}", path.display()))
}

fn show(file: &Path, preset: Option<&str>, raw: bool) -> Result<()> {
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

fn parse_selector(text: &str) -> Result<(u8, u8)> {
    let (bank, index) = text
        .split_once(':')
        .with_context(|| format!("expected bank:index, got {text:?}"))?;
    Ok((bank.trim().parse()?, index.trim().parse()?))
}

fn diff(before: &Path, after: &Path) -> Result<()> {
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

fn monitor(path: Option<&str>, timeout: i32) -> Result<()> {
    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    if let Some(build) = amp.handshake()? {
        println!("firmware build: {build}");
    }
    println!("listening (ctrl-c to stop)");
    loop {
        if let Some(report) = amp.read_report(timeout)? {
            match protocol::Message::parse(&report) {
                Some(message) => println!("{}  | {}", message.summary(), protocol::hex(&report)),
                None if report.starts_with(&protocol::END_OF_DUMP) => println!("ack  end of dump"),
                None => println!("raw  | {}", protocol::hex(&report)),
            }
        }
    }
}

fn learn(path: Option<&str>, interval: u64, count: Option<usize>) -> Result<()> {
    let amp = transport::Amp::open(path)?;
    println!("connected: {}", amp.describe()?);
    if let Some(build) = amp.handshake()? {
        println!("firmware build: {build}");
    }
    println!(
        "polling the live buffer every {}s; turn one knob at a time (ctrl-c to stop)\n",
        interval.max(1)
    );

    let mut previous: Option<Dump> = None;
    let mut round = 0usize;
    loop {
        let dump = amp.read_dump()?;
        round += 1;
        match (&previous, dump.live()) {
            (None, _) => println!("[{round}] baseline captured"),
            (Some(old), Some(new)) => {
                let changes = match old.live() {
                    Some(old_live) => protocol::diff_presets(old_live, new),
                    None => Vec::new(),
                };
                if changes.is_empty() {
                    println!("[{round}] no change");
                } else {
                    println!("[{round}] {} byte(s) changed", changes.len());
                    for change in &changes {
                        println!("    {}", change.line());
                    }
                }
            }
            (Some(_), None) => println!("[{round}] live buffer missing from dump"),
        }
        previous = Some(dump);
        if count.is_some_and(|limit| round >= limit) {
            break;
        }
        sleep(Duration::from_secs(interval.max(1)));
    }
    Ok(())
}

fn send(hex: &str, path: Option<&str>, yes: bool, execute: bool) -> Result<()> {
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
fn apply(
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
    let frame = protocol::as_host_frame(&raw, direction);

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
    if execute {
        let exec = protocol::execute_model_change();
        amp.write_report(&exec)?;
        println!("sent execute frame: {}", protocol::hex(&exec));
    }
    Ok(())
}

fn restore(
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

fn parse_value(text: &str) -> Result<u8> {
    let text = text.trim();
    let parsed = if let Some(hex) = text.strip_prefix("0x") {
        u8::from_str_radix(hex, 16)
    } else {
        text.parse::<u8>()
    };
    parsed.with_context(|| format!("value must be 0..255 or 0x00..0xff, got {text:?}"))
}

// ---------------------------------------------------------------------------
// rendering
// ---------------------------------------------------------------------------

fn print_dump(dump: &Dump) {
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

fn print_preset(preset: &Preset, raw: bool) {
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

fn cell(preset: &Preset, kind: Kind, width: usize) -> String {
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

fn bank_label(bank: u8) -> &'static str {
    match bank {
        BANK_LIVE => "live",
        BANK_AMP => "amp",
        BANK_FX => "fx",
        _ => "?",
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "â€¦"
}
