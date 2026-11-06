//! The preset library and the connection status under it.

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, Layout, Margin, Pos2, Sense, Ui, Vec2,
};

use super::paint::elided;
use super::store::StoreMode;
use super::theme::*;
use super::toolbar::{tool_button, ButtonKind};
use super::App;
use crate::models;
use crate::protocol::{self, Kind, BANK_AMP, BANK_FX, BANK_LIVE};

struct Row {
    key: (u8, u8),
    slot: String,
    name: String,
    detail: String,
}

impl App {
    fn rows(&self, bank: u8) -> Vec<Row> {
        self.dump
            .bank(bank)
            .map(|preset| {
                let live = bank == BANK_LIVE;
                let name = if live {
                    "Live Buffer".to_string()
                } else if preset.name.is_empty() {
                    format!("Preset {}", preset.index)
                } else {
                    preset.name.clone()
                };
                let chain = subtitle(preset);
                Row {
                    key: (preset.bank, preset.index),
                    slot: if live {
                        String::new()
                    } else {
                        preset.index.to_string()
                    },
                    // A detail line that only repeats the name is just noise.
                    detail: if chain == name { String::new() } else { chain },
                    name,
                }
            })
            .collect()
    }

    pub(super) fn sidebar(&mut self, ui: &mut Ui) {
        egui::TopBottomPanel::bottom("connection")
            .exact_height(if self.connected { 58.0 } else { 96.0 })
            .frame(
                egui::Frame::none()
                    .fill(SIDEBAR)
                    .inner_margin(Margin::symmetric(16.0, 10.0)),
            )
            .show_inside(ui, |ui| self.connection(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(SIDEBAR))
            .show_inside(ui, |ui| {
                let sections = [
                    ("Live", self.rows(BANK_LIVE)),
                    ("Amp Presets", self.rows(BANK_AMP)),
                    ("FX Presets", self.rows(BANK_FX)),
                ];
                let selected = self.selected;
                let mut clicked = None;
                let mut rename = None;
                let mut import = false;
                let mut save_clean = false;
                let mut save_voice = false;
                let mut save_effects = false;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.add_space(10.0);
                        for (title, rows) in &sections {
                            if rows.is_empty() {
                                continue;
                            }
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.add_space(14.0);
                                ui.label(
                                    egui::RichText::new(*title)
                                        .font(semibold(11.5))
                                        .color(TEXT_3),
                                );
                                if *title == "Live" {
                                    if section_action(
                                        ui,
                                        "Import…",
                                        "Copy modules from a preset into the live buffer",
                                    ) {
                                        import = true;
                                    }
                                    if section_action(
                                        ui,
                                        "Save CH1…",
                                        "Save channel 1 into the amp's Clean Channel slot",
                                    ) {
                                        save_clean = true;
                                    }
                                } else if *title == "Amp Presets" {
                                    if section_action(
                                        ui,
                                        "Save voice…",
                                        "Save the amp, cabinet and stomp into a voice slot",
                                    ) {
                                        save_voice = true;
                                    }
                                } else if *title == "FX Presets"
                                    && section_action(
                                        ui,
                                        "Save effects…",
                                        "Save mod, delay and reverb into an FX preset",
                                    )
                                {
                                    save_effects = true;
                                }
                            });
                            ui.add_space(4.0);
                            for row in rows {
                                let response = preset_row(ui, row, selected == row.key);
                                if response.clicked() {
                                    clicked = Some(row.key);
                                }
                                if row.key.0 != BANK_LIVE {
                                    response.context_menu(|ui| {
                                        if ui.button("Rename…").clicked() {
                                            rename = Some(row.key);
                                            ui.close_menu();
                                        }
                                    });
                                }
                            }
                        }
                        ui.add_space(10.0);
                    });
                if let Some(key) = clicked {
                    self.selected = key;
                }
                if let Some((bank, index)) = rename {
                    self.open_rename_dialog(bank, index);
                }
                if import {
                    self.open_import_dialog();
                }
                if save_clean {
                    self.open_sub_preset(StoreMode::Clean);
                }
                if save_voice {
                    self.open_sub_preset(StoreMode::Voice);
                }
                if save_effects {
                    self.open_sub_preset(StoreMode::Effects);
                }
            });
    }

    fn connection(&mut self, ui: &mut Ui) {
        let (title, color) = if self.connected {
            ("Connected", TEXT)
        } else if self.connecting {
            ("Connecting…", TEXT_2)
        } else {
            ("Not connected", TEXT_2)
        };
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(egui::RichText::new(title).font(semibold(13.0)).color(color));

        let detail = match (&self.banner, self.connected) {
            (Some(banner), true) => format!("Firmware {banner}"),
            _ => self.source.clone(),
        };
        let response = ui.add(
            egui::Label::new(
                egui::RichText::new(detail)
                    .font(regular(11.5))
                    .color(TEXT_3),
            )
            .truncate(true),
        );
        response.on_hover_text(self.source.clone());

        if !self.connected {
            ui.add_space(8.0);
            let enabled = !self.connecting;
            if tool_button(ui, "Connect", enabled, ButtonKind::Plain).clicked() {
                self.connect(None);
            }
        }
    }
}

fn preset_row(ui: &mut Ui, row: &Row, selected: bool) -> egui::Response {
    let has_detail = !row.detail.is_empty();
    let height = if has_detail { 44.0 } else { 34.0 };
    let (full, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let rect = full.shrink2(Vec2::new(8.0, 0.0));
    let painter = ui.painter();

    if selected {
        painter.rect_filled(rect, 7.0, AMBER.gamma_multiply(0.2));
    } else if response.hovered() {
        painter.rect_filled(rect, 7.0, Color32::from_white_alpha(9));
    }

    painter.text(
        Pos2::new(rect.left() + 22.0, rect.center().y),
        Align2::RIGHT_CENTER,
        &row.slot,
        regular(11.5),
        if selected { AMBER } else { TEXT_3 },
    );

    let x = rect.left() + 32.0;
    let width = rect.right() - x - 8.0;
    let name_y = if has_detail {
        rect.top() + 5.0
    } else {
        rect.top() + 7.0
    };
    let name = elided(
        painter,
        &row.name,
        if selected {
            semibold(13.5)
        } else {
            regular(13.5)
        },
        TEXT,
        width,
    );
    painter.galley(Pos2::new(x, name_y), name, TEXT);
    if has_detail {
        let detail = elided(painter, &row.detail, regular(11.0), TEXT_2, width);
        painter.galley(Pos2::new(x, rect.top() + 24.0), detail, TEXT_2);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn subtitle(preset: &protocol::Preset) -> String {
    let parts: Vec<&str> = [Kind::Amp, Kind::Mod, Kind::Delay, Kind::Reverb]
        .into_iter()
        .filter_map(|kind| {
            let model = preset.block(kind)?.model()?;
            (model != 0).then(|| models::model_name(kind, model).unwrap_or("?"))
        })
        .collect();
    parts.join(" · ")
}

/// A small action button on the right of a sidebar section header.
fn section_action(ui: &mut Ui, label: &str, tip: &str) -> bool {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.add_space(8.0);
        ui.small_button(label).on_hover_text(tip).clicked()
    })
    .inner
}
