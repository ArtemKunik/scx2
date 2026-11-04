//! Command line front end.

mod args;
mod device;
mod edit;
mod offline;
mod param;

use anyhow::Result;
use clap::Parser;

use crate::windows::{detach_console_if_owner, report_ui_error};

use self::args::{Cli, Command};
use self::device::{dump, learn, monitor, scan};
use self::edit::{apply, bypass, recall, rename, restore, send, store};
use self::offline::{diff, export, import, replay, show};
use self::param::{param, probe};

pub fn run() -> Result<()> {
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
        Command::Import { file, out } => import(&file, out),
        Command::Export { file, preset, out } => export(&file, &preset, &out),
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
        Command::Recall { slot, bank, path } => recall(bank, slot, path.as_deref()),
        Command::Store {
            bank,
            index,
            name,
            path,
            yes,
        } => store(bank, index, name.as_deref(), path.as_deref(), yes),
        Command::Rename {
            name,
            bank,
            index,
            path,
            yes,
        } => rename(&name, bank, index, path.as_deref(), yes),
        Command::Bypass {
            kind,
            slot,
            off,
            path,
            yes,
        } => bypass(&kind, slot, off, path.as_deref(), yes),
        Command::Param {
            index,
            value,
            selector,
            model,
            path,
            yes,
        } => param(
            index,
            value,
            selector,
            model.as_deref(),
            path.as_deref(),
            yes,
        ),
        Command::Probe {
            from,
            to,
            value,
            selector,
            path,
            yes,
        } => probe(from, to, value, selector, path.as_deref(), yes),
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
