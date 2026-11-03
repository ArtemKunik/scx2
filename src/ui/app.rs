//! Application state and the operations that are not pure drawing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::CreationContext;

use crate::capture;
use crate::protocol::{Dump, BANK_AMP, BANK_FX, BANK_LIVE};

use super::art::Art;
use super::sync::Activity;
use super::worker::{spawn_worker, Cmd, Ev};
use super::{theme, DEMO_CAPTURE};
// ---------------------------------------------------------------------------
// app state
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct Knob {
    pub(crate) offset: usize,
    pub(crate) label: String,
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
    ctx: egui::Context,
    pub(crate) connecting: bool,
    pub(crate) show_params: bool,
    pub(crate) show_fx_params: bool,
    pub(crate) art: Option<Rc<Art>>,
    /// Undo/redo snapshots of the whole dump.
    pub(crate) history: Vec<Dump>,
    pub(crate) future: Vec<Dump>,
    /// Consecutive edits inside this window become one undo step.
    pub(crate) last_edit: Option<Instant>,
    /// The file last opened or saved, if any.
    pub(crate) path: Option<PathBuf>,
}

impl App {
    pub(crate) fn new(
        cc: &CreationContext<'_>,
        from: Option<PathBuf>,
        path: Option<String>,
    ) -> Self {
        theme::install(&cc.egui_ctx);

        let mut app = App {
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
            ctx: cc.egui_ctx.clone(),
            connecting: false,
            show_params: false,
            show_fx_params: false,
            art: None,
            history: Vec::new(),
            future: Vec::new(),
            last_edit: None,
            path: None,
        };

        if let Some(file) = from {
            app.load_file(&file);
        } else {
            app.load_demo();
        }
        app.connect(path);
        app
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
                Ev::Warning(message) => self.error = Some(message),
                Ev::Error(message) => {
                    self.connected = false;
                    self.connecting = false;
                    self.status = "Not connected".into();
                    self.error = Some(message);
                    self.rx = None;
                }
            }
        }
        if self.connecting {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if !self.flash.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    pub(crate) fn elapsed_flash(&mut self, dt: f32) {
        for remaining in self.flash.values_mut() {
            *remaining -= dt;
        }
        self.flash.retain(|_, remaining| *remaining > 0.0);
    }
}
