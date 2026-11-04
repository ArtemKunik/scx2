//! Command line surface.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use scx2::protocol::{BANK_AMP, BANK_LIVE};

#[derive(Parser)]
#[command(
    name = "scx2",
    version,
    about = "Super Champ X2 preset librarian (unofficial)",
    long_about = "Talking to a Fender Super Champ X2 over USB: back up the preset memory, \
                  watch the amp live, map parameters and (once verified) write them back.\n\n\
                  Everything is read-only unless you pass --yes to a write command."
)]
pub(crate) struct Cli {
    /// Omit to open the desktop editor.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
    /// Import a Fender FUSE `.fuse` preset into a JSON backup
    Import {
        /// `.fuse` file written by Fender FUSE
        file: PathBuf,
        /// Where to write the one-preset JSON backup
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Export one preset from a backup as a Fender FUSE `.fuse` file
    Export {
        /// JSON file written by `scx2 dump`
        file: PathBuf,
        /// Preset to export, as `bank:index` (e.g. `1:7`)
        #[arg(long)]
        preset: String,
        /// Where to write the `.fuse` file
        #[arg(short, long)]
        out: PathBuf,
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
    /// Show a stored preset and the SCX2 recall limitation (read-only)
    Recall {
        /// Preset slot to show
        slot: u8,
        /// Bank to read (1 amp presets, 2 fx presets)
        #[arg(long, default_value_t = BANK_AMP)]
        bank: u8,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
    },
    /// Store the live buffer into a preset slot
    Store {
        /// Target bank (1 amp presets, 2 fx presets)
        #[arg(long, default_value_t = BANK_AMP)]
        bank: u8,
        /// Target preset index
        #[arg(long)]
        index: u8,
        /// Rename the stored preset as part of the store
        #[arg(long)]
        name: Option<String>,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Actually transmit
        #[arg(long)]
        yes: bool,
    },
    /// Rename a stored preset
    Rename {
        /// New name, at most 32 characters
        name: String,
        /// Bank holding the preset
        #[arg(long, default_value_t = BANK_AMP)]
        bank: u8,
        /// Preset index
        #[arg(long)]
        index: u8,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Actually transmit
        #[arg(long)]
        yes: bool,
    },
    /// Bypass or enable a loaded effect (verified frame)
    Bypass {
        /// Effect kind: stomp, mod, delay or reverb
        kind: String,
        /// Effect slot (the block's slot byte, 0-7)
        slot: u8,
        /// Bypass the effect (omit to enable it)
        #[arg(long)]
        off: bool,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Actually transmit
        #[arg(long)]
        yes: bool,
    },
    /// Send one direct parameter change (Mustang numbering)
    Param {
        /// Parameter index, 0 volume, 1 gain, 4 treble, 6 bass, ...
        index: u8,
        /// Value 0..127
        value: u8,
        /// Selector byte: 0x0c for amp parameters
        #[arg(long, default_value_t = scx2::params::SELECTOR_AMP)]
        selector: u8,
        /// Model id to address, e.g. 0x53 (defaults to the live voice)
        #[arg(long)]
        model: Option<String>,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Actually transmit
        #[arg(long)]
        yes: bool,
    },
    /// Sweep parameter indices and report which block offsets they move
    Probe {
        /// First parameter index
        #[arg(long, default_value_t = 0)]
        from: u8,
        /// Last parameter index
        #[arg(long, default_value_t = 20)]
        to: u8,
        /// Value to write while probing, 0..127
        #[arg(long, default_value_t = 127)]
        value: u8,
        /// Selector byte: 0x0c for amp parameters
        #[arg(long, default_value_t = scx2::params::SELECTOR_AMP)]
        selector: u8,
        /// HID path to open (see `scx2 scan`)
        #[arg(long)]
        path: Option<String>,
        /// Actually transmit (the amp is restored afterwards)
        #[arg(long)]
        yes: bool,
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
