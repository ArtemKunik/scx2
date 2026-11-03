//! Header, footer and the preset browser.

use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Sense, Ui, Vec2};

use crate::models;
use crate::protocol::{self, Kind, BANK_AMP, BANK_FX, BANK_LIVE};

use super::{stroke, App, ACCENT, ACCENT_DIM, BAD, EDITED, GOOD, HOVER, MUTED, TEXT};
// ---------------------------------------------------------------------------
// panels
// ---------------------------------------------------------------------------

impl App {
    pub(crate) fn header(&mut self, ui: &mut Ui) {
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

    pub(crate) fn footer(&mut self, ui: &mut Ui) {
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

    pub(crate) fn preset_list(&mut self, ui: &mut Ui) {
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
