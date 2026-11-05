//! Everything on screen: the amp is the interface.
//!
//! Hierarchy, top to bottom: the sound you are editing, the knobs that shape
//! it, what sits around it in the chain. Raw bytes stay one click away for
//! reverse-engineering, but they no longer compete with the instrument.

use eframe::egui::{self, Align2, Color32, Id, Margin, Ui};

use super::art::Art;
use super::cabinet::{panel_layout, CABINET_PAD};
use super::chain::PEDAL_H;
use super::theme::*;
use super::App;

const SIDEBAR_W: f32 = 252.0;

const CONTENT_MAX_W: f32 = 1240.0;

const GRILLE_MIN: f32 = 72.0;

const GRILLE_MAX: f32 = 380.0;

pub(super) fn show(app: &mut App, ctx: &egui::Context, art: &Art) {
    egui::SidePanel::left("presets")
        .exact_width(SIDEBAR_W)
        .resizable(false)
        .frame(egui::Frame::none().fill(SIDEBAR))
        .show(ctx, |ui| app.sidebar(ui));

    egui::TopBottomPanel::top("toolbar")
        .exact_height(44.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::none()
                .fill(WINDOW)
                .inner_margin(Margin::symmetric(24.0, 12.0)),
        )
        .show(ctx, |ui| app.toolbar(ui));

    egui::TopBottomPanel::bottom("status")
        .exact_height(30.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::none()
                .fill(WINDOW)
                .inner_margin(Margin::symmetric(24.0, 0.0)),
        )
        .show(ctx, |ui| app.status_bar(ui));

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(WINDOW).inner_margin(Margin {
            left: 24.0,
            right: 24.0,
            top: 0.0,
            bottom: 4.0,
        }))
        .show(ctx, |ui| app.body(ui, art));

    drop_overlay(ctx);
}

fn drop_overlay(ctx: &egui::Context) {
    if ctx.input(|i| i.raw.hovered_files.is_empty()) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("drop-overlay"),
    ));
    let screen = ctx.screen_rect();
    painter.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
    painter.rect_stroke(
        screen.shrink(14.0),
        14.0,
        stroke(2.0, AMBER.gamma_multiply(0.8)),
    );
    painter.text(
        screen.center(),
        Align2::CENTER_CENTER,
        "Drop to open a backup or .fuse preset",
        semibold(22.0),
        TEXT,
    );
}

impl App {
    fn body(&mut self, ui: &mut Ui, art: &Art) {
        let key = self.selected;
        let Some(preset) = self.dump.get(key.0, key.1).cloned() else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("Choose a preset")
                        .font(regular(15.0))
                        .color(TEXT_2),
                );
            });
            return;
        };

        let viewport = ui.available_height();
        let width = ui.available_width().min(CONTENT_MAX_W);
        let (_, panel_h) = panel_layout(width, &self.knobs);

        // The grille takes the room that is left, and yields when the
        // parameters open. Cloth is the one thing here that can give way.
        let tuner_h = if self.show_tuner { 244.0 } else { 0.0 };
        let recorder_h = if self.show_recorder { 156.0 } else { 0.0 };
        let fixed = panel_h
            + 2.0 * CABINET_PAD
            + 14.0
            + 18.0
            + PEDAL_H
            + 14.0
            + tuner_h
            + recorder_h
            + 28.0
            + 8.0;
        let target = if self.show_params || self.show_fx_params {
            GRILLE_MIN
        } else {
            (viewport - fixed).clamp(GRILLE_MIN, GRILLE_MAX)
        };
        let grille_h = ui
            .ctx()
            .animate_value_with_time(Id::new("grille-height"), target, 0.22);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let pad = ((ui.available_width() - width) / 2.0).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        self.amp(ui, art, &preset, width, panel_h, grille_h);
                        ui.add_space(18.0);
                        self.pedals(ui, art, &preset, width);
                        if self.show_tuner {
                            ui.add_space(14.0);
                            self.tuner_panel(ui, width);
                        }
                        if self.show_recorder {
                            ui.add_space(14.0);
                            self.recorder_panel(ui, width);
                        }
                        ui.add_space(14.0);
                        self.effect_parameters(ui, &preset, width);
                        ui.add_space(8.0);
                        self.parameters(ui, &preset, width);
                    });
                });
            });
    }
}
