//! The toolbar over the amp and the status line under it.

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, Id, Layout, Pos2, Rect, Sense, Ui, Vec2,
};

use super::paint::elided;
use super::theme::*;
use super::worker::Cmd;
use super::App;
use crate::protocol::{Kind, BANK_AMP, BANK_FX, BANK_LIVE};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ButtonKind {
    Plain,
    Primary,
    Active,
}

pub(super) fn tool_button(
    ui: &mut Ui,
    label: &str,
    enabled: bool,
    kind: ButtonKind,
) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_string(), semibold(12.5), Color32::PLACEHOLDER);
    let size = Vec2::new(galley.size().x + 26.0, 30.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();

    let (fill, ink) = match (kind, enabled) {
        (ButtonKind::Primary, true) => (
            if pressed {
                Color32::from_rgb(226, 152, 56)
            } else if hovered {
                Color32::from_rgb(255, 193, 104)
            } else {
                AMBER
            },
            AMBER_INK,
        ),
        (ButtonKind::Active, _) => (AMBER.gamma_multiply(0.22), AMBER),
        (_, false) => (Color32::from_white_alpha(6), TEXT_3),
        (_, true) => (
            Color32::from_white_alpha(if pressed {
                34
            } else if hovered {
                24
            } else {
                14
            }),
            TEXT,
        ),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 7.0, fill);
    painter.galley(rect.center() - galley.size() / 2.0, galley, ink);
    response
}

impl App {
    /// Changes as a person counts them. Voice is stored in three bytes that
    /// always move together, so it counts once.
    fn pending_count(&self) -> usize {
        self.pending()
            .iter()
            .filter(|change| !(change.kind == Kind::Amp && matches!(change.offset, 45 | 46)))
            .count()
    }

    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let key = self.selected;
        let preset = self.dump.get(key.0, key.1);
        let title = match preset {
            Some(_) if key.0 == BANK_LIVE => "Live Buffer".to_string(),
            Some(preset) if !preset.name.is_empty() => preset.name.clone(),
            Some(preset) => format!("Preset {}", preset.index),
            None => "No preset selected".to_string(),
        };
        let place = match key.0 {
            BANK_LIVE => "What the amp is playing now".to_string(),
            BANK_AMP => format!("Amp preset {}", key.1),
            BANK_FX => format!("FX preset {}", key.1),
            _ => String::new(),
        };

        let pending = self.pending_count();
        ui.horizontal_centered(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(egui::RichText::new(title).font(semibold(20.0)).color(TEXT));
                ui.label(egui::RichText::new(place).font(regular(11.5)).color(TEXT_2));
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = match pending {
                    0 => "Write to Amp".to_string(),
                    1 => "Write 1 Change".to_string(),
                    count => format!("Write {count} Changes"),
                };
                let can_write = pending > 0 && self.connected;
                let kind = if can_write {
                    ButtonKind::Primary
                } else {
                    ButtonKind::Plain
                };
                let write = tool_button(ui, &label, can_write, kind);
                if write.clicked() {
                    self.write_pending();
                }
                if pending > 0 && !self.connected {
                    write.on_hover_text("Connect the amp to write these changes.");
                } else if pending == 0 {
                    write.on_hover_text("Nothing to write yet. Turn a knob first.");
                }
                if pending > 0 && tool_button(ui, "Revert", true, ButtonKind::Plain).clicked() {
                    self.dump = self.original.clone();
                    self.status = "Changes reverted".into();
                }

                ui.add_space(12.0);

                let learn_kind = if self.polling {
                    ButtonKind::Active
                } else {
                    ButtonKind::Plain
                };
                let learn_label = if self.polling {
                    "Stop Learning"
                } else {
                    "Learn"
                };
                let learn =
                    tool_button(ui, learn_label, self.connected || self.polling, learn_kind);
                let learn_clicked = learn.clicked();
                learn.on_hover_text("Turn a knob on the amp to see which parameter it moves.");
                if learn_clicked {
                    self.toggle_poll();
                }

                if tool_button(ui, "Save Backup", true, ButtonKind::Plain).clicked() {
                    self.save_backup();
                }
                let refresh = tool_button(ui, "Refresh", self.connected, ButtonKind::Plain);
                if refresh.clicked() {
                    if let Some(tx) = &self.tx {
                        let _ = tx.send(Cmd::Refresh);
                        self.status = "Reading presets from the amp…".into();
                    }
                }
                refresh.on_hover_text("Read every preset from the amp again.");
            });
        });
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        let y = rect.center().y;

        let Some(error) = self.error.clone() else {
            painter.text(
                Pos2::new(rect.right(), y),
                Align2::RIGHT_CENTER,
                "Drop a backup anywhere to open it",
                regular(11.5),
                TEXT_3,
            );
            let galley = elided(
                &painter,
                &self.status,
                regular(11.5),
                TEXT_3,
                rect.width() * 0.6,
            );
            let at = Pos2::new(rect.left(), y - galley.size().y / 2.0);
            painter.galley(at, galley, TEXT_3);
            return;
        };

        let cross = Pos2::new(rect.left() + 6.0, y);
        painter.text(cross, Align2::CENTER_CENTER, "×", regular(14.0), ERROR);
        let dismiss = ui
            .interact(
                Rect::from_center_size(cross, Vec2::splat(18.0)),
                Id::new("dismiss-error"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("Dismiss");

        // First line here; troubleshooting hints live in the tooltip.
        let headline = error.lines().next().unwrap_or_default();
        let galley = elided(
            &painter,
            headline,
            regular(11.5),
            ERROR,
            rect.width() - 20.0,
        );
        let area = Rect::from_min_size(
            Pos2::new(rect.left() + 18.0, y - galley.size().y / 2.0),
            galley.size(),
        );
        painter.galley(area.min, galley, ERROR);
        ui.interact(area, Id::new("error-text"), Sense::hover())
            .on_hover_text(error.clone());

        if dismiss.clicked() {
            self.error = None;
        }
    }
}
