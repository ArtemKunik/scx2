//! Desktop UI (egui), started by `scx2 ui`.

mod ampstyle;
mod app;
mod art;
mod cabinet;
mod chain;
mod dial;
mod edits;
mod fx;
mod hardware;
mod knobs;
mod paint;
mod parameters;
mod parts;
mod units;
mod pedal;
mod sidebar;
mod sync;
mod texture;
mod theme;
mod toolbar;
mod view;
mod worker;

use std::path::PathBuf;
use std::rc::Rc;

use eframe::egui;

pub(crate) use app::App;

/// Built-in demo material, so the UI is usable before the amp is connected.
const DEMO_CAPTURE: &[u8] = include_bytes!("../../captures/fuse-scx2-2021-06-07.pcap");

/// Rebuild the procedural materials when the pixel density changes, so they
/// stay 1:1 with physical pixels.
fn current_art(app: &mut App, ctx: &egui::Context) -> Rc<art::Art> {
    let ppp = ctx.pixels_per_point();
    match &app.art {
        Some(art) if art.ppp == ppp => art.clone(),
        _ => {
            let art = Rc::new(art::Art::new(ctx, ppp));
            app.art = Some(art.clone());
            art
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.elapsed_flash(dt);
        self.pump(ctx);
        self.active_offset = None;
        handle_drops(self, ctx);
        let art = current_art(self, ctx);
        view::show(self, ctx, &art);
        self.send_live_edits();
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
// entry point
// ---------------------------------------------------------------------------

pub fn run(from: Option<PathBuf>, path: Option<String>) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([900.0, 600.0])
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
