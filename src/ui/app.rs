//! Application state and the operations that are not pure drawing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use eframe::egui;
use eframe::CreationContext;

use crate::capture;
use crate::protocol::{self, Dump, BANK_AMP, BANK_FX, BANK_LIVE};

use super::worker::{spawn_worker, Cmd, Ev};
use super::{install_style, DEMO_CAPTURE};
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
    pub(crate) polling: bool,
    pub(crate) last_live: Option<Dump>,
    pub(crate) flash: BTreeMap<usize, f32>,
    pub(crate) learn_log: Vec<String>,
    pub(crate) connecting: bool,
}

impl App {
    pub(crate) fn new(
        cc: &CreationContext<'_>,
        from: Option<PathBuf>,
        path: Option<String>,
    ) -> Self {
        install_style(&cc.egui_ctx);

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
            status: "offline".into(),
            error: None,
            polling: false,
            last_live: None,
            flash: BTreeMap::new(),
            learn_log: Vec::new(),
            connecting: false,
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
                self.source = "bundled 2021 FUSE capture".into();
                self.status = "demo capture loaded".into();
                self.pick_default_preset();
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(crate) fn load_file(&mut self, path: &Path) {
        match std::fs::read_to_string(path)
            .map_err(anyhow::Error::from)
            .and_then(|text| Dump::from_json(&text).map_err(anyhow::Error::from))
        {
            Ok(dump) => {
                self.dump = dump;
                self.original = self.dump.clone();
                self.source = path.display().to_string();
                self.status = format!("loaded {}", path.display());
                self.pick_default_preset();
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(crate) fn pick_default_preset(&mut self) {
        let wanted = [(BANK_AMP, 1u8), (BANK_LIVE, 0), (BANK_FX, 0)];
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
        spawn_worker(path, ev_tx, cmd_rx);
        self.tx = Some(cmd_tx);
        self.rx = Some(ev_rx);
        self.connecting = true;
        self.status = "connecting...".into();
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
                    self.source = "SuperChamp X2  [1ed8:000e]".into();
                    self.status = "connected".into();
                }
                Ev::Dump(dump) => {
                    self.handle_live_dump(dump);
                }
                Ev::Written(count) => {
                    self.original = self.dump.clone();
                    self.status = format!("sent {count} report(s)");
                }
                Ev::Error(message) => {
                    self.connected = false;
                    self.connecting = false;
                    self.status = "offline".into();
                    self.error = Some(message);
                    self.rx = None;
                }
            }
        }
        if self.connecting {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        if self.polling {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if !self.flash.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    pub(crate) fn handle_live_dump(&mut self, dump: Dump) {
        if self.polling {
            if let (Some(previous), Some(current)) =
                (self.last_live.as_ref().and_then(Dump::live), dump.live())
            {
                let changes = protocol::diff_presets(previous, current);
                for change in &changes {
                    if change.offset >= protocol::OFF_PARAMS {
                        self.flash.insert(change.offset, 1.4);
                    }
                }
                if !changes.is_empty() {
                    let mut line = String::new();
                    for change in changes.iter().take(6) {
                        line.push_str(&format!("{}  ", change.line()));
                    }
                    self.learn_log.push(line);
                    if self.learn_log.len() > 8 {
                        self.learn_log.remove(0);
                    }
                }
            }
            self.last_live = Some(dump.clone());
            // Do not clobber local edits while polling.
            if protocol::diff_dumps(&self.original, &self.dump).is_empty() {
                self.dump = dump;
                self.original = self.dump.clone();
            }
            return;
        }

        self.dump = dump;
        self.original = self.dump.clone();
        self.last_live = Some(self.dump.clone());
        self.pick_default_preset();
    }

    pub(crate) fn toggle_poll(&mut self) {
        self.polling = !self.polling;
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Poll(self.polling));
        }
        if self.polling {
            self.learn_log.clear();
            self.status = "learning: turn one knob on the amp".into();
        } else {
            self.status = "learning stopped".into();
        }
    }

    pub(crate) fn elapsed_flash(&mut self, dt: f32) {
        for remaining in self.flash.values_mut() {
            *remaining -= dt;
        }
        self.flash.retain(|_, remaining| *remaining > 0.0);
    }
}
