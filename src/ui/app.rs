//! Application state and the operations that are not pure drawing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::CreationContext;

use crate::capture;
use crate::midi as midi_input;
use crate::protocol::{Dump, BANK_AMP, BANK_FX, BANK_LIVE};
use crate::recorder::Recorder;
use crate::tuner::Tuner;

use super::art::Art;
use super::rename::RenameDialog;
use super::restore::RestoreDialog;
use super::store::StoreDialog;
use super::sync::Activity;
use super::worker::{spawn_worker, Cmd, Ev};
use super::{theme, DEMO_CAPTURE};
// ---------------------------------------------------------------------------
// app state
// ---------------------------------------------------------------------------

/// How long to wait for the amp's read-back after a save or rename frame.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Clone)]
pub(crate) struct Knob {
    pub(crate) offset: usize,
    pub(crate) label: String,
}

/// A live-buffer byte the person changed that the amp has not echoed yet.
///
/// A dump can overtake a write, so its copy of the live buffer may be older
/// than what is on screen. Keep the local value until the amp reports it back
/// (or until it is old enough that the write must have been ignored).
pub(crate) struct PendingLive {
    pub(crate) value: u8,
    pub(crate) seen: Instant,
}

pub struct App {
    pub(crate) dump: Dump,
    pub(crate) original: Dump,
    pub(crate) selected: (u8, u8),
    pub(crate) source: String,
    pub(crate) knobs: Vec<Knob>,
    pub(crate) tx: Option<Sender<Cmd>>,
    pub(crate) rx: Option<Receiver<Ev>>,
    pub(crate) connected: bool,
    pub(crate) banner: Option<String>,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) flash: BTreeMap<usize, f32>,
    pub(crate) activity: Vec<Activity>,
    /// The parameter a person is dragging right now; the amp must not move it.
    pub(crate) active_offset: Option<usize>,
    /// False until the first dump from the amp arrives.
    pub(crate) synced: bool,
    pub(crate) ctx: egui::Context,
    pub(crate) connecting: bool,
    pub(crate) show_params: bool,
    pub(crate) show_fx_params: bool,
    pub(crate) show_tuner: bool,
    pub(crate) show_recorder: bool,
    /// Which effect slot is open in the focused editor, if any.
    pub(crate) fx_editor: Option<crate::protocol::Kind>,
    /// Whether that editor is showing the model list instead of its controls.
    pub(crate) fx_editor_list: bool,
    /// False on the frame the editor opens, so the click that opened it cannot
    /// also dismiss it through the backdrop.
    pub(crate) fx_editor_armed: bool,
    /// Whether the amp/speaker picker prefers to open above the grille plate.
    pub(crate) picker_above: bool,
    pub(crate) tuner: Tuner,
    pub(crate) recorder: Recorder,
    /// A running attempt to switch the amp's USB input on.
    pub(crate) amp_job: Option<crate::endpoint::EnableJob>,
    /// What the last attempt reported, shown under the tuner header.
    pub(crate) amp_note: Option<String>,
    pub(crate) art: Option<Rc<Art>>,
    /// Undo/redo snapshots of the whole dump.
    pub(crate) history: Vec<Dump>,
    pub(crate) future: Vec<Dump>,
    /// Live buffer edits waiting for the amp to echo them back.
    pub(crate) pending_live: BTreeMap<(u8, usize), PendingLive>,
    /// Consecutive edits inside this window become one undo step.
    pub(crate) last_edit: Option<Instant>,
    /// The file last opened or saved, if any.
    pub(crate) path: Option<PathBuf>,
    /// The "Save to Amp…" dialog, when it is open.
    pub(crate) store: Option<StoreDialog>,
    /// The "Rename Preset" dialog, when it is open.
    pub(crate) rename: Option<RenameDialog>,
    /// The "Restore Backup" dialog, when it is open or running.
    pub(crate) restore: Option<RestoreDialog>,
    /// A device operation the worker has not confirmed yet, and what it was.
    pub(crate) waiting: Option<(Instant, &'static str)>,
    /// When the last full dump arrived, shown in the save dialog.
    pub(crate) synced_at: Option<Instant>,
    /// MIDI foot-controller settings, the open input and its event channel.
    pub(crate) midi: midi_input::Config,
    pub(crate) midi_router: midi_input::Router,
    pub(crate) midi_listener: Option<midi_input::Listener>,
    pub(crate) midi_rx: Option<Receiver<midi_input::Event>>,
    pub(crate) midi_ports: Vec<String>,
    pub(crate) midi_error: Option<String>,
    /// The effect waiting for the next control to be moved, if any.
    pub(crate) midi_learn: Option<midi_input::Effect>,
    pub(crate) show_midi: bool,
}

impl App {
    pub(crate) fn new(
        cc: &CreationContext<'_>,
        from: Option<PathBuf>,
        path: Option<String>,
    ) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = App::fresh(cc.egui_ctx.clone());
        let (midi, midi_error) = midi_input::Config::load();
        app.midi = midi;
        app.midi_router.set_config(app.midi.clone());
        app.midi_error = midi_error;
        app.start_midi();
        if let Some(file) = from {
            app.load_file(&file);
        } else {
            app.load_demo();
        }
        app.connect(path);
        app
    }

    /// An app with no capture, no amp and no file: the state the UI (and the
    /// tests) start from.
    pub(super) fn fresh(ctx: egui::Context) -> Self {
        App {
            dump: Dump::new(),
            original: Dump::new(),
            selected: (BANK_AMP, 1),
            source: String::new(),
            knobs: vec![
                Knob {
                    offset: 32,
                    label: "Volume".into(),
                },
                Knob {
                    offset: 33,
                    label: "Gain".into(),
                },
                Knob {
                    offset: 36,
                    label: "Treble".into(),
                },
                Knob {
                    offset: 37,
                    label: "Middle".into(),
                },
                Knob {
                    offset: 38,
                    label: "Bass".into(),
                },
                Knob {
                    offset: 44,
                    label: "Voice".into(),
                },
            ],
            tx: None,
            rx: None,
            connected: false,
            banner: None,
            status: "Not connected".into(),
            error: None,
            flash: BTreeMap::new(),
            activity: Vec::new(),
            active_offset: None,
            synced: false,
            ctx,
            connecting: false,
            show_params: false,
            show_fx_params: false,
            show_tuner: false,
            show_recorder: false,
            fx_editor: None,
            fx_editor_list: false,
            fx_editor_armed: false,
            picker_above: false,
            tuner: Tuner::new(),
            recorder: Recorder::new(),
            amp_job: None,
            amp_note: None,
            art: None,
            history: Vec::new(),
            future: Vec::new(),
            pending_live: BTreeMap::new(),
            last_edit: None,
            path: None,
            store: None,
            rename: None,
            restore: None,
            waiting: None,
            synced_at: None,
            midi: midi_input::Config::default(),
            midi_router: midi_input::Router::new(midi_input::Config::default()),
            midi_listener: None,
            midi_rx: None,
            midi_ports: Vec::new(),
            midi_error: None,
            midi_learn: None,
            show_midi: false,
        }
    }

    pub(crate) fn load_demo(&mut self) {
        match capture::amp_reports_from_bytes(DEMO_CAPTURE) {
            Ok(reports) => {
                self.dump = Dump::from_reports(reports);
                self.original = self.dump.clone();
                self.source = "Demo capture from 2021".into();
                self.status = "Demo presets loaded".into();
                self.path = None;
                self.history.clear();
                self.future.clear();
                self.pending_live.clear();
                self.last_edit = None;
                self.pick_default_preset();
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    /// Open a JSON backup or a single-preset FUSE `.fuse` file.
    pub(crate) fn load_file(&mut self, path: &Path) {
        let is_fuse = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("fuse"));
        let parsed = std::fs::read_to_string(path)
            .map_err(anyhow::Error::from)
            .and_then(|text| {
                if is_fuse {
                    crate::fuse::parse(&text).map(Dump::single)
                } else {
                    Dump::from_json(&text).map_err(anyhow::Error::from)
                }
            });
        match parsed {
            Ok(dump) => {
                self.dump = dump;
                self.original = self.dump.clone();
                self.source = path.display().to_string();
                self.path = Some(path.to_path_buf());
                self.history.clear();
                self.future.clear();
                self.pending_live.clear();
                self.last_edit = None;
                self.status = format!("Opened {}", path.display());
                self.pick_default_preset();
                if is_fuse && self.connected {
                    self.send_live_preset();
                    self.status =
                        format!("Opened {} and sent it to the live buffer", path.display());
                }
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(crate) fn pick_default_preset(&mut self) {
        let wanted = [(BANK_LIVE, 0u8), (BANK_AMP, 1), (BANK_FX, 0)];
        for key in wanted {
            if self.dump.get(key.0, key.1).is_some() {
                self.selected = key;
                return;
            }
        }
        if let Some(first) = self.dump.presets().next() {
            self.selected = (first.bank, first.index);
        }
    }

    pub(crate) fn connect(&mut self, path: Option<String>) {
        let (ev_tx, ev_rx) = channel();
        let (cmd_tx, cmd_rx) = channel();
        spawn_worker(path, ev_tx, cmd_rx, self.ctx.clone());
        self.tx = Some(cmd_tx);
        self.rx = Some(ev_rx);
        self.connecting = true;
        self.synced = false;
        self.synced_at = None;
        self.pending_live.clear();
        self.error = None;
        self.status = "Connecting…".into();
    }

    pub(crate) fn pump(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        for event in events {
            match event {
                Ev::Connected(banner) => {
                    self.connected = true;
                    self.connecting = false;
                    self.banner = banner;
                    self.source = "Super Champ X2 (1ed8:000e)".into();
                    self.status = "Connected. Knobs follow the amp.".into();
                }
                Ev::Dump(dump) => self.on_dump(dump),
                Ev::Live(live) => self.on_live(*live),
                Ev::Knob { offset, value } => self.on_knob(offset, value),
                Ev::StoreDone {
                    bank,
                    index,
                    outcome,
                }
                | Ev::RenameDone {
                    bank,
                    index,
                    outcome,
                } => self.finish_operation(bank, index, outcome),
                Ev::RestoreProgress { done, total } => self.on_restore_progress(done, total),
                Ev::RestoreDone(report) => self.on_restore_done(report),
                Ev::Warning(message) => self.error = Some(message),
                Ev::Error(message) => {
                    let stopped_midway = self
                        .restore
                        .as_ref()
                        .is_some_and(|dialog| dialog.is_running());
                    self.connected = false;
                    self.connecting = false;
                    self.waiting = None;
                    self.restore = None;
                    self.status = "Not connected".into();
                    self.error = Some(if stopped_midway {
                        format!(
                            "{message}\nThe restore stopped mid-way. Check the slots it had already written."
                        )
                    } else {
                        message
                    });
                    self.rx = None;
                }
            }
        }
        if self.connecting {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some((sent, what)) = self.waiting {
            if sent.elapsed() >= OPERATION_TIMEOUT {
                self.waiting = None;
                self.error = Some(format!(
                    "The amp did not confirm the {what}. Check the slot with a fresh dump before trusting it."
                ));
            } else {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        if self.restore_stalled() {
            self.restore = None;
            self.error = Some(
                "The amp stopped reporting during the restore. Check the slots it had already written before trying again."
                    .into(),
            );
        }
        if self
            .restore
            .as_ref()
            .is_some_and(|dialog| dialog.is_running())
        {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if !self.flash.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    /// Show the slot an operation wrote, now that the dump has the amp's own
    /// copy of it, and report what the read-back found.
    fn finish_operation(&mut self, bank: u8, index: u8, outcome: Result<String, String>) {
        self.waiting = None;
        self.selected = (bank, index);
        match outcome {
            Ok(summary) => self.status = summary,
            Err(message) => self.error = Some(message),
        }
    }

    pub(crate) fn elapsed_flash(&mut self, dt: f32) {
        for remaining in self.flash.values_mut() {
            *remaining -= dt;
        }
        self.flash.retain(|_, remaining| *remaining > 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_app() -> (egui::Context, App) {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let mut app = App::fresh(ctx.clone());
        let reports =
            capture::amp_reports_from_bytes(DEMO_CAPTURE).expect("the demo capture parses");
        app.dump = Dump::from_reports(reports);
        app.original = app.dump.clone();
        app.synced = true;
        app.synced_at = Some(Instant::now());
        (ctx, app)
    }

    /// Render the save dialog headlessly; a panic here is a broken dialog.
    fn render(ctx: &egui::Context, app: &mut App) {
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.store_window(ctx));
        }
    }

    fn render_rename(ctx: &egui::Context, app: &mut App) {
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.rename_window(ctx));
        }
    }

    #[test]
    fn the_save_dialog_opens_on_the_selected_preset() {
        let (ctx, mut app) = demo_app();
        app.selected = (BANK_AMP, 3);
        app.open_store_dialog();
        assert!(app.store.is_some());
        render(&ctx, &mut app);
        assert!(app.store.is_some(), "the dialog stays open while idle");
    }

    #[test]
    fn the_save_dialog_opens_on_an_empty_slot_too() {
        let (ctx, mut app) = demo_app();
        if let Some(preset) = app.dump.get_mut(BANK_AMP, 5) {
            preset.name.clear();
        }
        app.selected = (BANK_AMP, 5);
        app.open_store_dialog();
        render(&ctx, &mut app);
        assert!(app.store.is_some());
    }

    #[test]
    fn a_dry_run_reports_the_frame_and_writes_nothing() {
        let (_, mut app) = demo_app();
        app.connected = false;
        app.open_store_dialog();
        let dialog = app.store.clone().expect("the dialog is open");
        assert!(app.commit_store(&dialog));
        assert!(app.status.starts_with("Dry run: "), "{}", app.status);
        assert!(app.status.contains("1c 01 03"), "{}", app.status);
    }

    #[test]
    fn the_rename_dialog_opens_on_a_named_slot() {
        let (ctx, mut app) = demo_app();
        app.open_rename_dialog(BANK_AMP, 3);
        assert!(app.rename.is_some(), "amp 3 has a name");
        render_rename(&ctx, &mut app);
        assert!(app.rename.is_some(), "the dialog stays open while idle");
    }

    #[test]
    fn an_empty_slot_has_nothing_to_rename() {
        let (_, mut app) = demo_app();
        if let Some(preset) = app.dump.get_mut(BANK_AMP, 3) {
            preset.name.clear();
        }
        app.open_rename_dialog(BANK_AMP, 3);
        assert!(app.rename.is_none());
    }

    #[test]
    fn a_rename_dry_run_reports_the_name_frame() {
        let (_, mut app) = demo_app();
        app.connected = false;
        app.open_rename_dialog(BANK_AMP, 3);
        let mut dialog = app.rename.clone().expect("the dialog is open");
        assert!(app.commit_rename(&mut dialog));
        assert!(app.status.starts_with("Dry run: "), "{}", app.status);
        assert!(app.status.contains("1c c3"), "{}", app.status);
        assert!(app.status.contains("1c 03"), "{}", app.status);
    }
}
