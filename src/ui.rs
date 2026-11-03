//! Desktop UI (egui), started by `scx2 ui`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use eframe::CreationContext;

use crate::protocol::{self, Dump, Kind, BANK_AMP, BANK_FX, BANK_LIVE};
use crate::{capture, models, transport};

// ---------------------------------------------------------------------------
// palette
// ---------------------------------------------------------------------------

const BG: Color32 = Color32::from_rgb(15, 16, 20);
const PANEL: Color32 = Color32::from_rgb(24, 26, 32);
const PANEL_HI: Color32 = Color32::from_rgb(32, 35, 43);
const HOVER: Color32 = Color32::from_rgb(42, 46, 56);
const TEXT: Color32 = Color32::from_rgb(232, 234, 238);
const MUTED: Color32 = Color32::from_rgb(138, 146, 160);
const ACCENT: Color32 = Color32::from_rgb(232, 176, 75);
const ACCENT_DIM: Color32 = Color32::from_rgb(120, 92, 40);
const GOOD: Color32 = Color32::from_rgb(96, 208, 140);
const BAD: Color32 = Color32::from_rgb(226, 106, 106);
const EDITED: Color32 = Color32::from_rgb(93, 156, 236);
const TOLEX: Color32 = Color32::from_rgb(13, 14, 17);
const SILVER: Color32 = Color32::from_rgb(198, 203, 211);
const SILVER_DARK: Color32 = Color32::from_rgb(128, 135, 146);
const PLATE_INK: Color32 = Color32::from_rgb(22, 24, 28);
const JEWEL: Color32 = Color32::from_rgb(233, 76, 60);

/// Built-in demo material, so the UI is usable before the amp is connected.
const DEMO_CAPTURE: &[u8] = include_bytes!("../captures/fuse-scx2-2021-06-07.pcap");

// ---------------------------------------------------------------------------
// background worker
// ---------------------------------------------------------------------------

enum Cmd {
    Refresh,
    Poll(bool),
    Write(Vec<[u8; protocol::REPORT_SIZE]>),
}

enum Ev {
    Connected(Option<String>),
    Dump(Dump),
    Written(usize),
    Error(String),
}

fn spawn_worker(path: Option<String>, tx: Sender<Ev>, rx: Receiver<Cmd>) {
    // The device is opened inside the thread, so nothing has to be Send.
    thread::spawn(move || {
        let amp = match transport::Amp::open(path.as_deref()) {
            Ok(amp) => amp,
            Err(error) => {
                let _ = tx.send(Ev::Error(format!("{error:#}")));
                return;
            }
        };
        let banner = amp.handshake().ok().flatten();
        let _ = tx.send(Ev::Connected(banner));
        match amp.read_dump() {
            Ok(dump) => {
                let _ = tx.send(Ev::Dump(dump));
            }
            Err(error) => {
                let _ = tx.send(Ev::Error(format!("{error:#}")));
                return;
            }
        }

        let mut polling = false;
        loop {
            let command = if polling {
                rx.recv_timeout(Duration::from_millis(600))
            } else {
                rx.recv_timeout(Duration::from_millis(250))
            };
            match command {
                Ok(Cmd::Refresh) => match amp.read_dump() {
                    Ok(dump) => {
                        let _ = tx.send(Ev::Dump(dump));
                    }
                    Err(error) => {
                        let _ = tx.send(Ev::Error(format!("{error:#}")));
                        break;
                    }
                },
                Ok(Cmd::Poll(on)) => {
                    polling = on;
                    if on {
                        if let Ok(dump) = amp.read_dump() {
                            let _ = tx.send(Ev::Dump(dump));
                        }
                    }
                }
                Ok(Cmd::Write(frames)) => {
                    let mut sent = 0;
                    for frame in frames {
                        if amp.write_report(&frame).is_err() {
                            break;
                        }
                        // The amp buffers a block write until the execute frame.
                        let _ = amp.write_report(&protocol::execute_model_change());
                        sent += 1;
                        thread::sleep(Duration::from_millis(25));
                    }
                    let _ = tx.send(Ev::Written(sent));
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    if polling {
                        if let Ok(dump) = amp.read_dump() {
                            let _ = tx.send(Ev::Dump(dump));
                        }
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// app state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Knob {
    offset: usize,
    label: String,
}

pub struct App {
    dump: Dump,
    original: Dump,
    selected: (u8, u8),
    source: String,
    knobs: Vec<Knob>,
    tx: Option<Sender<Cmd>>,
    rx: Option<Receiver<Ev>>,
    connected: bool,
    banner: Option<String>,
    status: String,
    error: Option<String>,
    polling: bool,
    last_live: Option<Dump>,
    flash: BTreeMap<usize, f32>,
    learn_log: Vec<String>,
    connecting: bool,
}

impl App {
    fn new(cc: &CreationContext<'_>, from: Option<PathBuf>, path: Option<String>) -> Self {
        install_style(&cc.egui_ctx);

        let mut app = App {
            dump: Dump::new(),
            original: Dump::new(),
            selected: (BANK_AMP, 1),
            source: String::new(),
            knobs: vec![Knob {
                offset: 44,
                label: "Voice".into(),
            }],
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

    fn load_demo(&mut self) {
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

    fn load_file(&mut self, path: &Path) {
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

    fn pick_default_preset(&mut self) {
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

    fn connect(&mut self, path: Option<String>) {
        let (ev_tx, ev_rx) = channel();
        let (cmd_tx, cmd_rx) = channel();
        spawn_worker(path, ev_tx, cmd_rx);
        self.tx = Some(cmd_tx);
        self.rx = Some(ev_rx);
        self.connecting = true;
        self.status = "connecting...".into();
    }

    fn pump(&mut self, ctx: &egui::Context) {
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

    fn handle_live_dump(&mut self, dump: Dump) {
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

    fn toggle_poll(&mut self) {
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

    fn pending(&self) -> Vec<protocol::Change> {
        protocol::diff_dumps(&self.original, &self.dump)
    }

    fn write_pending(&mut self) {
        let mut blocks: BTreeSet<(u8, u8, u8)> = BTreeSet::new();
        for change in self.pending() {
            blocks.insert((change.bank, change.index, change.kind.code()));
        }
        let frames: Vec<[u8; protocol::REPORT_SIZE]> = blocks
            .into_iter()
            .filter_map(|(bank, index, code)| {
                self.dump
                    .get(bank, index)?
                    .block(Kind::from_code(code))
                    .map(|message| {
                        protocol::as_host_frame(message.as_bytes(), protocol::HOST_CONTROL)
                    })
            })
            .collect();
        if frames.is_empty() {
            return;
        }
        match &self.tx {
            Some(tx) if self.connected => {
                let _ = tx.send(Cmd::Write(frames.clone()));
                self.status = format!("sending {} report(s)...", frames.len());
            }
            _ => {
                self.error = Some(
                    "no amp connected: the changes are kept locally, connect to send them".into(),
                );
            }
        }
    }

    fn save_backup(&mut self) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = PathBuf::from(format!("scx2-backup-{stamp}.json"));
        match serde_json::to_string_pretty(&self.dump.to_json())
            .map_err(anyhow::Error::from)
            .and_then(|text| std::fs::write(&path, text).map_err(anyhow::Error::from))
        {
            Ok(()) => self.status = format!("saved {}", path.display()),
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    fn apply_edit(&mut self, kind: Kind, offset: usize, value: u8) {
        let (bank, index) = self.selected;
        if let Some(preset) = self.dump.get_mut(bank, index) {
            if let Some(block) = preset.block_mut(kind) {
                block.set_byte(offset, value);
            }
        }
        // Voice is stored three times; keep the copies in sync.
        if kind == Kind::Amp && (44..=46).contains(&offset) {
            if let Some(preset) = self.dump.get_mut(bank, index) {
                if let Some(block) = preset.block_mut(Kind::Amp) {
                    for other in [44usize, 45, 46] {
                        block.set_byte(other, value);
                    }
                }
            }
        }
    }

    fn elapsed_flash(&mut self, dt: f32) {
        for remaining in self.flash.values_mut() {
            *remaining -= dt;
        }
        self.flash.retain(|_, remaining| *remaining > 0.0);
    }
}

fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke { width, color }
}

fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(12, 13, 16);
    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.inactive.bg_fill = PANEL_HI;
    style.visuals.widgets.inactive.weak_bg_fill = PANEL_HI;
    style.visuals.widgets.hovered.bg_fill = HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = HOVER;
    style.visuals.widgets.active.bg_fill = ACCENT_DIM;
    style.visuals.selection.bg_fill = ACCENT_DIM;
    style.visuals.selection.stroke = stroke(1.0, ACCENT);
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    ctx.set_style(style);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.elapsed_flash(dt);
        self.pump(ctx);

        handle_drops(self, ctx);

        egui::TopBottomPanel::top("header")
            .exact_height(74.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(18.0, 12.0)),
            )
            .show(ctx, |ui| self.header(ui));

        egui::TopBottomPanel::bottom("footer")
            .exact_height(58.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(18.0, 10.0)),
            )
            .show(ctx, |ui| self.footer(ui));

        egui::SidePanel::left("presets")
            .exact_width(282.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(12.0, 12.0)),
            )
            .show(ctx, |ui| self.preset_list(ui));

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::same(18.0)),
            )
            .show(ctx, |ui| self.body(ui));
    }
}

fn handle_drops(app: &mut App, ctx: &egui::Context) {
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect()
    });
    if let Some(path) = dropped.first() {
        if path.extension().is_some_and(|ext| ext == "json") {
            app.load_file(path);
        }
    }
}

// ---------------------------------------------------------------------------
// panels
// ---------------------------------------------------------------------------

impl App {
    fn header(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("SUPER CHAMP X2")
                        .font(FontId::proportional(24.0))
                        .color(TEXT)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new("preset librarian")
                        .font(FontId::proportional(11.0))
                        .color(MUTED),
                );
            });

            ui.add_space(18.0);
            let (color, label) = if self.connected {
                (GOOD, "connected")
            } else if self.connecting {
                (ACCENT, "connecting")
            } else {
                (BAD, "offline")
            };
            status_pill(ui, color, label);
            if let Some(banner) = &self.banner {
                ui.label(
                    egui::RichText::new(format!("fw {banner}"))
                        .font(FontId::proportional(11.0))
                        .color(MUTED),
                );
            }

            let spacer = (ui.available_width() - 104.0).max(8.0);
            ui.add_space(spacer);
            if ui
                .add_enabled(
                    !self.connected && !self.connecting,
                    egui::Button::new("Connect").min_size(Vec2::new(92.0, 28.0)),
                )
                .clicked()
            {
                self.connect(None);
            }
        });
    }

    fn footer(&mut self, ui: &mut Ui) {
        let pending = self.pending().len();
        ui.horizontal(|ui| {
            let text = if pending == 0 {
                "no pending edits".to_string()
            } else {
                format!("{pending} pending byte(s)")
            };
            ui.label(
                egui::RichText::new(text)
                    .font(FontId::proportional(12.0))
                    .color(if pending == 0 { MUTED } else { EDITED }),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let write = ui.add_enabled(
                    pending > 0 && self.connected,
                    egui::Button::new(egui::RichText::new("Write to amp").strong())
                        .fill(ACCENT_DIM)
                        .rounding(9.0),
                );
                if write.clicked() {
                    self.write_pending();
                }
                if ui
                    .add_enabled(pending > 0, egui::Button::new("Revert"))
                    .clicked()
                {
                    self.dump = self.original.clone();
                    self.status = "changes reverted".into();
                }
                let learn_label = if self.polling {
                    "Stop learning"
                } else {
                    "Learn"
                };
                if ui
                    .add_enabled(
                        self.connected || self.polling,
                        egui::Button::new(learn_label),
                    )
                    .clicked()
                {
                    self.toggle_poll();
                }
                ui.label(
                    egui::RichText::new(&self.status)
                        .font(FontId::proportional(11.0))
                        .color(MUTED),
                );
            });
        });
        if let Some(error) = &self.error {
            ui.label(
                egui::RichText::new(error)
                    .font(FontId::proportional(11.0))
                    .color(BAD),
            );
        }
    }

    fn preset_list(&mut self, ui: &mut Ui) {
        ui.label(
            egui::RichText::new(self.source.clone())
                .font(FontId::proportional(10.5))
                .color(MUTED),
        );
        ui.add_space(6.0);

        let live: Vec<(u8, u8, String, String)> = self
            .dump
            .bank(BANK_LIVE)
            .map(|p| (p.bank, p.index, "Live buffer".to_string(), subtitle(p)))
            .collect();
        let amp: Vec<(u8, u8, String, String)> = self
            .dump
            .bank(BANK_AMP)
            .map(|p| {
                (
                    p.bank,
                    p.index,
                    if p.name.is_empty() {
                        format!("Preset {}", p.index)
                    } else {
                        p.name.clone()
                    },
                    subtitle(p),
                )
            })
            .collect();
        let fx: Vec<(u8, u8, String, String)> = self
            .dump
            .bank(BANK_FX)
            .map(|p| {
                (
                    p.bank,
                    p.index,
                    if p.name.is_empty() {
                        format!("Preset {}", p.index)
                    } else {
                        p.name.clone()
                    },
                    subtitle(p),
                )
            })
            .collect();

        let selected = self.selected;
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (title, rows) in [("LIVE", &live), ("AMP PRESETS", &amp), ("FX PRESETS", &fx)] {
                    section_heading(ui, title);
                    for (bank, index, name, sub) in rows {
                        if preset_row(ui, name, sub, selected == (*bank, *index)).clicked() {
                            clicked = Some((*bank, *index));
                        }
                    }
                }
            });
        if let Some(key) = clicked {
            self.selected = key;
        }
    }
}

fn section_heading(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(title)
            .font(FontId::proportional(10.0))
            .color(ACCENT)
            .strong(),
    );
    ui.add_space(2.0);
}

fn preset_row(ui: &mut Ui, name: &str, sub: &str, selected: bool) -> egui::Response {
    let height = 48.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 9.0, ACCENT_DIM);
        painter.rect_stroke(rect, 9.0, stroke(1.0, ACCENT));
    } else if response.hovered() {
        painter.rect_filled(rect, 9.0, HOVER);
    }
    let name_color = if selected {
        TEXT
    } else {
        Color32::from_rgb(206, 210, 218)
    };
    painter.text(
        rect.left_top() + Vec2::new(12.0, 9.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(14.0),
        name_color,
    );
    painter.text(
        rect.left_top() + Vec2::new(12.0, 28.0),
        Align2::LEFT_TOP,
        sub,
        FontId::proportional(10.5),
        MUTED,
    );
    response
}

fn subtitle(preset: &protocol::Preset) -> String {
    let mut parts = Vec::new();
    for kind in [Kind::Amp, Kind::Mod, Kind::Delay, Kind::Reverb] {
        if let Some(model) = preset.block(kind).and_then(|m| m.model()) {
            if model == 0 {
                continue;
            }
            let name = models::model_name(kind, model).unwrap_or("?");
            parts.push(name.to_string());
        }
    }
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(" - ")
    }
}

fn status_pill(ui: &mut Ui, color: Color32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(92.0, 24.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 12.0, color.gamma_multiply(0.18));
    ui.painter()
        .circle_filled(rect.left_center() + Vec2::new(14.0, 0.0), 4.0, color);
    ui.painter().text(
        rect.left_center() + Vec2::new(26.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.5),
        color,
    );
}

impl App {
    fn body(&mut self, ui: &mut Ui) {
        let key = self.selected;
        let Some(preset) = self.dump.get(key.0, key.1).cloned() else {
            ui.label("select a preset");
            return;
        };

        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                if let Some(tx) = &self.tx {
                    let _ = tx.send(Cmd::Refresh);
                    self.status = "requesting dump...".into();
                }
            }
            if ui.button("Save backup").clicked() {
                self.save_backup();
            }
            if ui.button("Load...").clicked() {
                self.status = "drop a .json onto the window".into();
            }
            ui.label(
                egui::RichText::new("drop a backup on the window to open it")
                    .font(FontId::proportional(10.0))
                    .color(MUTED),
            );
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if preset.name.is_empty() {
                    "Live buffer".to_string()
                } else {
                    preset.name.clone()
                })
                .font(FontId::proportional(22.0))
                .color(TEXT)
                .strong(),
            );
            ui.label(
                egui::RichText::new(format!("bank {} - slot {}", bank_name(key.0), key.1))
                    .font(FontId::proportional(11.0))
                    .color(MUTED),
            );
        });
        ui.add_space(10.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.amp_card(ui, &preset);
                ui.add_space(16.0);
                self.effects(ui, &preset);
                if self.polling || !self.learn_log.is_empty() {
                    ui.add_space(16.0);
                    self.learn_panel(ui);
                }
            });
    }

    fn amp_card(&mut self, ui: &mut Ui, preset: &protocol::Preset) {
        let Some(amp) = preset.block(Kind::Amp) else {
            return;
        };
        let mut frame = *amp.as_bytes();
        let original = frame;
        let model = amp.model().unwrap_or(0);
        let voice = models::model_name(Kind::Amp, model).unwrap_or("unknown voice");
        let connected = self.connected;

        egui::Frame::none()
            .fill(TOLEX)
            .rounding(16.0)
            .stroke(stroke(1.0, Color32::from_rgb(44, 38, 33)))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                let (plate, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 62.0), Sense::hover());
                paint_faceplate(ui, plate, voice, connected);
                ui.add_space(14.0);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("CONTROLS")
                            .font(FontId::proportional(9.5))
                            .color(Color32::from_rgb(120, 124, 134))
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("+ knob").clicked() {
                            self.add_knob();
                        }
                    });
                });
                ui.add_space(2.0);

                ui.horizontal_wrapped(|ui| {
                    let knobs = self.knobs.clone();
                    for knob in &knobs {
                        let offset = knob.offset;
                        if offset >= protocol::REPORT_SIZE {
                            continue;
                        }
                        let sub = knob_value_label(knob.offset, frame[offset]);
                        let range = if knob.offset == 44 { 13.0 } else { 255.0 };
                        let response = knob_widget(
                            ui,
                            &mut frame[offset],
                            &knob.label,
                            &sub,
                            range,
                            if knob.offset == 44 { ACCENT } else { EDITED },
                        );
                        if response.secondary_clicked() {
                            self.knobs.retain(|k| k.offset != knob.offset);
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "drag up/down to change - right-click a knob to unbind - \
                         parameters start at offset 32",
                    )
                    .font(FontId::proportional(10.0))
                    .color(Color32::from_rgb(120, 124, 134)),
                );

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("PARAMETERS")
                            .font(FontId::proportional(9.5))
                            .color(Color32::from_rgb(120, 124, 134))
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("32..63 - 0x80 is centre")
                            .font(FontId::proportional(9.5))
                            .color(Color32::from_rgb(96, 100, 110)),
                    );
                });
                ui.add_space(6.0);

                let mut bind_request = None;
                let columns = (((ui.available_width() + 8.0) / 52.0).floor() as usize).clamp(4, 8);
                for row_start in (0..32).step_by(columns) {
                    ui.horizontal(|ui| {
                        for index in row_start..(row_start + columns).min(32) {
                            let offset = protocol::OFF_PARAMS + index;
                            let flash = self.flash.get(&offset).copied().unwrap_or(0.0);
                            let response =
                                param_cell(ui, offset, &mut frame[offset], original[offset], flash);
                            if response.secondary_clicked() {
                                bind_request = Some(offset);
                            }
                        }
                    });
                }
                if let Some(offset) = bind_request {
                    self.add_knob_at(offset);
                }
            });

        if frame != original {
            for (offset, value) in frame.iter().enumerate() {
                if *value != original[offset] {
                    self.apply_edit(Kind::Amp, offset, *value);
                }
            }
        }
    }

    fn add_knob(&mut self) {
        for offset in protocol::OFF_PARAMS..protocol::REPORT_SIZE {
            if !self.knobs.iter().any(|k| k.offset == offset) {
                self.add_knob_at(offset);
                return;
            }
        }
    }

    fn add_knob_at(&mut self, offset: usize) {
        if self.knobs.iter().any(|k| k.offset == offset) {
            return;
        }
        if self.knobs.len() >= 10 {
            return;
        }
        self.knobs.push(Knob {
            offset,
            label: format!("P{offset}"),
        });
    }

    fn effects(&mut self, ui: &mut Ui, preset: &protocol::Preset) {
        ui.label(
            egui::RichText::new("EFFECTS")
                .font(FontId::proportional(10.0))
                .color(ACCENT)
                .strong(),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (kind, title) in [
                (Kind::Stomp, "STOMP"),
                (Kind::Mod, "MOD"),
                (Kind::Delay, "DELAY"),
                (Kind::Reverb, "REVERB"),
            ] {
                effect_card(ui, title, preset, kind);
            }
        });
    }

    fn learn_panel(&mut self, ui: &mut Ui) {
        egui::Frame::none()
            .fill(PANEL)
            .rounding(14.0)
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("LEARN")
                        .font(FontId::proportional(10.0))
                        .color(ACCENT)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "Turn one knob on the amp. The offsets that moved are listed here and \
                         flash in the parameter grid. Right-click a cell to bind it to a knob.",
                    )
                    .font(FontId::proportional(11.0))
                    .color(MUTED),
                );
                if self.learn_log.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("waiting for a change...")
                            .font(FontId::proportional(11.0))
                            .color(MUTED),
                    );
                }
                for line in &self.learn_log {
                    ui.label(
                        egui::RichText::new(line)
                            .font(FontId::monospace(11.0))
                            .color(TEXT),
                    );
                }
            });
    }
}

fn bank_name(bank: u8) -> &'static str {
    match bank {
        BANK_LIVE => "live",
        BANK_AMP => "amp",
        BANK_FX => "fx",
        _ => "?",
    }
}

fn knob_value_label(offset: usize, value: u8) -> String {
    if offset == 44 {
        models::model_name(Kind::Amp, voice_model_id(value))
            .unwrap_or("?")
            .to_string()
    } else {
        format!("0x{value:02x}  {value}")
    }
}

fn voice_model_id(index: u8) -> u16 {
    match index {
        0x00 => 0x00eb,
        0x01 => 0x0067,
        0x02 => 0x0064,
        0x03 => 0x0053,
        0x04 => 0x006a,
        0x05 => 0x0075,
        0x06 => 0x0072,
        0x07 => 0x0061,
        0x08 => 0x006d,
        0x09 => 0x005e,
        0x0a => 0x005d,
        0x0b => 0x0079,
        0x0c => 0x007c,
        0x0d => 0x00ea,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// widgets
// ---------------------------------------------------------------------------

fn paint_faceplate(ui: &Ui, rect: Rect, voice: &str, connected: bool) {
    let painter = ui.painter();

    // brushed metal: a flat fill, a bright top edge and a dark bottom edge
    painter.rect_filled(rect, 9.0, SILVER);
    let inner = rect.shrink(1.0);
    painter.line_segment(
        [
            inner.left_top() + Vec2::new(6.0, 1.0),
            inner.right_top() + Vec2::new(-6.0, 1.0),
        ],
        stroke(1.0, Color32::from_rgb(236, 239, 244)),
    );
    painter.line_segment(
        [
            inner.left_bottom() + Vec2::new(6.0, -1.0),
            inner.right_bottom() + Vec2::new(-6.0, -1.0),
        ],
        stroke(1.0, SILVER_DARK),
    );
    painter.rect_stroke(rect, 9.0, stroke(1.0, Color32::from_rgb(96, 102, 112)));

    // pilot jewel
    let jewel = rect.left_center() + Vec2::new(24.0, 0.0);
    let glow = if connected {
        JEWEL
    } else {
        Color32::from_rgb(70, 40, 38)
    };
    painter.circle_filled(jewel, 11.0, glow.gamma_multiply(0.25));
    painter.circle_filled(jewel, 6.5, glow);
    painter.circle_filled(
        jewel - Vec2::new(1.5, 1.5),
        2.5,
        glow.gamma_multiply(1.0).linear_multiply(0.8),
    );
    painter.circle_stroke(jewel, 7.5, stroke(1.0, Color32::from_rgb(60, 64, 72)));

    painter.text(
        rect.left_center() + Vec2::new(46.0, -9.0),
        Align2::LEFT_CENTER,
        "SUPER CHAMP X2",
        FontId::proportional(21.0),
        PLATE_INK,
    );
    painter.text(
        rect.left_center() + Vec2::new(48.0, 13.0),
        Align2::LEFT_CENTER,
        "FENDER  -  TUBE AMPLIFIER",
        FontId::proportional(8.5),
        Color32::from_rgb(60, 64, 72),
    );

    painter.text(
        rect.right_center() + Vec2::new(-18.0, -8.0),
        Align2::RIGHT_CENTER,
        voice,
        FontId::proportional(16.0),
        PLATE_INK,
    );
    painter.text(
        rect.right_center() + Vec2::new(-18.0, 12.0),
        Align2::RIGHT_CENTER,
        "VOICE",
        FontId::proportional(8.5),
        Color32::from_rgb(60, 64, 72),
    );
}

fn knob_widget(
    ui: &mut Ui,
    value: &mut u8,
    label: &str,
    sub: &str,
    range: f32,
    accent: Color32,
) -> egui::Response {
    let size = Vec2::new(92.0, 108.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if response.dragged() {
        let pixels = -response.drag_delta().y + response.drag_delta().x;
        let delta = pixels * (range / 160.0);
        let next = (*value as f32 + delta).clamp(0.0, range).round() as u8;
        if next != *value {
            *value = next;
            response.changed = true;
        }
    }

    let center = rect.center_top() + Vec2::new(0.0, 50.0);
    let radius = 27.0;
    let start = 135f32.to_radians();
    let end = 405f32.to_radians();
    let t = (*value as f32 / range).clamp(0.0, 1.0);
    let painter = ui.painter();

    painter.circle_filled(
        center + Vec2::new(0.0, 3.0),
        radius + 7.0,
        Color32::from_rgb(6, 7, 9),
    );
    painter.circle_filled(center, radius + 6.0, Color32::from_rgb(16, 17, 21));
    painter.circle_stroke(
        center,
        radius + 6.0,
        stroke(1.0, Color32::from_rgb(48, 51, 60)),
    );
    for tick in 0..11 {
        let angle = start + (end - start) * tick as f32 / 10.0;
        let dir = Vec2::new(angle.cos(), angle.sin());
        painter.line_segment(
            [center + dir * (radius + 2.5), center + dir * (radius + 5.0)],
            stroke(1.0, Color32::from_rgb(74, 79, 90)),
        );
    }

    let track = arc_points(center, radius + 10.0, start, end);
    painter.add(egui::Shape::line(
        track,
        stroke(4.0, Color32::from_rgb(46, 50, 60)),
    ));
    if t > 0.005 {
        let arc = arc_points(center, radius + 10.0, start, start + (end - start) * t);
        painter.add(egui::Shape::line(arc, stroke(4.0, accent)));
    }

    // knob body: a few concentric circles fake a soft bevel
    painter.circle_filled(center, radius, Color32::from_rgb(22, 24, 29));
    painter.circle_filled(
        center - Vec2::new(0.0, 1.0),
        radius - 2.0,
        Color32::from_rgb(36, 39, 47),
    );
    painter.circle_filled(
        center - Vec2::new(0.0, 3.0),
        radius - 7.0,
        Color32::from_rgb(46, 50, 60),
    );
    painter.circle_stroke(center, radius, stroke(1.0, Color32::from_rgb(58, 63, 74)));

    let angle = start + (end - start) * t;
    let dir = Vec2::new(angle.cos(), angle.sin());
    painter.line_segment(
        [
            center + dir * (radius * 0.3),
            center + dir * (radius * 0.82),
        ],
        stroke(3.0, Color32::WHITE),
    );

    if response.hovered() {
        painter.circle_stroke(
            center,
            radius + 14.0,
            stroke(1.0, accent.gamma_multiply(0.5)),
        );
    }

    painter.text(
        rect.center_top() + Vec2::new(0.0, 2.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(11.0),
        MUTED,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 88.0),
        Align2::CENTER_TOP,
        sub,
        FontId::proportional(10.5),
        if response.hovered() { TEXT } else { MUTED },
    );

    ui.advance_cursor_after_rect(rect);
    response.on_hover_text(format!("{label} - {sub}"))
}

fn arc_points(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = 48;
    (0..=steps)
        .map(|i| {
            let angle = from + (to - from) * i as f32 / steps as f32;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

fn param_cell(
    ui: &mut Ui,
    offset: usize,
    value: &mut u8,
    original: u8,
    flash: f32,
) -> egui::Response {
    let size = Vec2::new(44.0, 52.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if response.dragged() {
        let delta = (-response.drag_delta().y + response.drag_delta().x) * 1.2;
        let next = (*value as f32 + delta).clamp(0.0, 255.0).round() as u8;
        if next != *value {
            *value = next;
            response.changed = true;
        }
    }

    let painter = ui.painter();
    let edited = *value != original;
    let bg = if edited {
        Color32::from_rgb(28, 38, 52)
    } else if response.hovered() {
        HOVER
    } else {
        PANEL_HI
    };
    painter.rect_filled(rect, 7.0, bg);
    if edited {
        painter.rect_stroke(rect, 7.0, stroke(1.0, EDITED));
    }

    let fill_height = rect.height() * (*value as f32 / 255.0);
    let fill = Rect::from_min_max(
        Pos2::new(rect.left() + 4.0, rect.bottom() - fill_height),
        Pos2::new(rect.right() - 4.0, rect.bottom() - 4.0),
    );
    if fill.height() > 1.0 {
        painter.rect_filled(fill, 4.0, ACCENT.gamma_multiply(0.22));
    }

    if flash > 0.0 {
        let alpha = (flash / 1.4).clamp(0.0, 1.0);
        painter.rect_stroke(rect, 7.0, stroke(2.0, ACCENT.gamma_multiply(alpha)));
    }

    painter.text(
        rect.center_top() + Vec2::new(0.0, 6.0),
        Align2::CENTER_TOP,
        format!("{offset}"),
        FontId::proportional(9.5),
        MUTED,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 20.0),
        Align2::CENTER_TOP,
        format!("{:02x}", value),
        FontId::monospace(13.0),
        if edited { EDITED } else { TEXT },
    );

    ui.advance_cursor_after_rect(rect);
    response.on_hover_text(format!(
        "offset {offset} - 0x{value:02x} ({value}) - right-click to bind a knob"
    ))
}

fn effect_card(ui: &mut Ui, title: &str, preset: &protocol::Preset, kind: Kind) {
    let width = 214.0;
    let height = 92.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, PANEL);
    painter.rect_stroke(rect, 12.0, stroke(1.0, Color32::from_rgb(40, 44, 53)));

    let model = preset.block(kind).and_then(|m| m.model()).unwrap_or(0);
    let loaded = model != 0;
    painter.text(
        rect.left_top() + Vec2::new(14.0, 12.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(9.5),
        ACCENT,
    );
    let name = if loaded {
        models::model_name(kind, model).unwrap_or("unknown")
    } else {
        "-"
    };
    painter.text(
        rect.left_top() + Vec2::new(14.0, 30.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(15.0),
        if loaded { TEXT } else { MUTED },
    );
    if let Some(slot) = preset.block(kind).and_then(|m| m.slot()) {
        painter.text(
            rect.left_bottom() + Vec2::new(14.0, -14.0),
            Align2::LEFT_BOTTOM,
            format!("slot {slot}"),
            FontId::proportional(10.0),
            MUTED,
        );
    }
    if loaded {
        painter.text(
            rect.right_bottom() + Vec2::new(-14.0, -14.0),
            Align2::RIGHT_BOTTOM,
            "on",
            FontId::proportional(10.0),
            GOOD,
        );
    }
}

// ---------------------------------------------------------------------------
// entry point
// ---------------------------------------------------------------------------

pub fn run(from: Option<PathBuf>, path: Option<String>) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([900.0, 560.0])
            .with_maximized(true)
            .with_title("Super Champ X2"),
        ..Default::default()
    };
    eframe::run_native(
        "scx2",
        options,
        Box::new(move |cc| Box::new(App::new(cc, from, path))),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}
