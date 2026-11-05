//! Commands that talk to a connected amp: scan, dump, monitor, learn.

use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;

use scx2::protocol::{self, Dump};
use scx2::transport;

use super::offline::{default_dump_name, print_dump, write_dump};

pub(crate) fn scan(all: bool) -> Result<()> {
    let devices = transport::scan(!all)?;
    if devices.is_empty() {
        println!("no devices found");
        if !all {
            println!("hint: `scx2 scan --all` lists every HID device");
        }
        #[cfg(target_os = "linux")]
        println!(
            "hint: Linux exposes HID devices as /dev/hidraw*; if none are listed, check the \
             cable and install packaging/linux/99-scx2.rules"
        );
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

pub(crate) fn dump(path: Option<&str>, out: Option<PathBuf>) -> Result<()> {
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

pub(crate) fn monitor(path: Option<&str>, timeout: i32) -> Result<()> {
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

pub(crate) fn learn(path: Option<&str>, interval: u64, count: Option<usize>) -> Result<()> {
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
