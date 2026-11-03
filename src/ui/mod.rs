//! Desktop UI (egui), started by `scx2 ui`.

mod amp;
mod app;
mod edits;
mod faceplate;
mod knobs;
mod panels;
mod widgets;
mod worker;

use std::path::PathBuf;

use eframe::egui::{self, Color32, Stroke, Vec2};

use crate::protocol::{BANK_AMP, BANK_FX, BANK_LIVE};

pub(crate) use app::App;

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
const DEMO_CAPTURE: &[u8] = include_bytes!("../../captures/fuse-scx2-2021-06-07.pcap");

pub(crate) fn stroke(width: f32, color: Color32) -> egui::Stroke {
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

fn bank_name(bank: u8) -> &'static str {
    match bank {
        BANK_LIVE => "live",
        BANK_AMP => "amp",
        BANK_FX => "fx",
        _ => "?",
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
