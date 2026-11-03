//! The raw parameter grid and recent changes, behind a disclosure.

use eframe::egui::{
    self, Align2, CursorIcon, FontId, Id, Margin, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2,
};

use super::dial::drag_value;
use super::knobs::add_knob;
use super::theme::*;
use super::App;
use crate::params;
use crate::protocol::{self, Kind, BANK_LIVE};

impl App {
    pub(super) fn parameters(&mut self, ui: &mut Ui, preset: &protocol::Preset, width: f32) {
        let openness =
            ui.ctx()
                .animate_bool_with_time(Id::new("params-open"), self.show_params, 0.16);

        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
        let painter = ui.painter();
        let ink = if response.hovered() { TEXT } else { TEXT_2 };
        let tip = rect.left_center() + Vec2::new(6.0, 0.0);
        let (sin, cos) = (openness * std::f32::consts::FRAC_PI_2).sin_cos();
        let turn = |x: f32, y: f32| tip + Vec2::new(x * cos - y * sin, x * sin + y * cos);
        painter.add(Shape::convex_polygon(
            vec![turn(-3.0, -4.5), turn(4.0, 0.0), turn(-3.0, 4.5)],
            ink,
            Stroke::NONE,
        ));
        painter.text(
            rect.left_center() + Vec2::new(18.0, 0.0),
            Align2::LEFT_CENTER,
            "Parameters",
            semibold(12.5),
            ink,
        );
        painter.text(
            rect.right_center(),
            Align2::RIGHT_CENTER,
            "Raw amp block, offsets 32–63",
            regular(11.5),
            TEXT_3,
        );
        if response.clicked() {
            self.show_params = !self.show_params;
        }
        response.on_hover_cursor(CursorIcon::PointingHand);

        if !self.show_params {
            return;
        }
        ui.add_space(8.0);
        self.parameter_grid(ui, preset, width);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "Drag a cell on the Live Buffer to change it. Right-click to add it to the panel.",
            )
            .font(regular(11.5))
            .color(TEXT_3),
        );
        if !self.activity.is_empty() {
            ui.add_space(14.0);
            self.activity_panel(ui);
        }
        ui.add_space(8.0);
    }

    fn parameter_grid(&mut self, ui: &mut Ui, preset: &protocol::Preset, width: f32) {
        const GAP: f32 = 6.0;
        let Some(block) = preset.block(Kind::Amp) else {
            return;
        };
        let mut frame = *block.as_bytes();
        let before = frame;
        let baseline = self.baseline(preset);

        let editable = !self.connected || preset.bank == BANK_LIVE;
        let cell_width = |columns: usize| (width - GAP * (columns as f32 - 1.0)) / columns as f32;
        let columns = [16usize, 8, 4]
            .into_iter()
            .find(|columns| cell_width(*columns) >= 56.0)
            .unwrap_or(4);
        let cell_w = cell_width(columns);

        let mut bind = None;
        ui.spacing_mut().item_spacing = Vec2::splat(GAP);
        for start in (0..32).step_by(columns) {
            ui.horizontal(|ui| {
                for index in start..(start + columns).min(32) {
                    let offset = protocol::OFF_PARAMS + index;
                    let flash = self.flash.get(&offset).copied().unwrap_or(0.0);
                    let response = param_cell(
                        ui,
                        cell_w,
                        offset,
                        &mut frame[offset],
                        baseline[offset],
                        flash,
                        editable,
                    );
                    if response.dragged() {
                        self.active_offset = Some(offset);
                    }
                    if response.secondary_clicked() {
                        bind = Some(offset);
                    }
                }
            });
        }
        ui.spacing_mut().item_spacing = Vec2::ZERO;

        if let Some(offset) = bind {
            add_knob(&mut self.knobs, offset);
        }
        self.commit(&frame, &before);
    }

    fn activity_panel(&mut self, ui: &mut Ui) {
        egui::Frame::none()
            .fill(SIDEBAR)
            .rounding(10.0)
            .inner_margin(Margin::same(14.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(
                    egui::RichText::new("Recent Changes")
                        .font(semibold(13.0))
                        .color(TEXT),
                );
                for entry in &self.activity {
                    ui.label(
                        egui::RichText::new(entry.line())
                            .font(FontId::monospace(11.5))
                            .color(TEXT),
                    );
                }
            });
    }
}

fn param_cell(
    ui: &mut Ui,
    width: f32,
    offset: usize,
    value: &mut u8,
    baseline: u8,
    flash: f32,
    editable: bool,
) -> egui::Response {
    let sense = if editable {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 58.0), sense);
    drag_value(ui, &response, value, 255.0, 1.2);

    let painter = ui.painter();
    let edited = *value != baseline;
    let fill = if response.hovered() { RAISED } else { SIDEBAR };
    painter.rect_filled(rect, 7.0, fill);

    painter.text(
        rect.left_top() + Vec2::new(8.0, 6.0),
        Align2::LEFT_TOP,
        offset.to_string(),
        regular(10.5),
        TEXT_3,
    );
    painter.text(
        rect.center() + Vec2::new(0.0, -2.0),
        Align2::CENTER_CENTER,
        format!("{value:02x}", value = *value),
        FontId::monospace(14.0),
        if edited { AMBER } else { TEXT },
    );
    painter.text(
        rect.center_bottom() + Vec2::new(0.0, -8.0),
        Align2::CENTER_BOTTOM,
        params::short_offset_label(offset),
        regular(9.5),
        TEXT_2,
    );

    // Level as a hairline on the floor of the cell.
    let track = Rect::from_min_max(
        Pos2::new(rect.left() + 6.0, rect.bottom() - 4.0),
        Pos2::new(rect.right() - 6.0, rect.bottom() - 2.5),
    );
    painter.rect_filled(track, 1.0, HAIRLINE);
    let mut level = track;
    level.set_right(track.left() + track.width() * (*value as f32 / 255.0));
    painter.rect_filled(level, 1.0, if edited { AMBER } else { TEXT_2 });

    if flash > 0.0 {
        let alpha = (flash / 1.4).clamp(0.0, 1.0);
        painter.rect_stroke(rect, 7.0, stroke(2.0, AMBER.gamma_multiply(alpha)));
    }

    let name = params::offset_label(offset)
        .map(|label| format!("{label}, "))
        .unwrap_or_default();
    response
        .on_hover_cursor(CursorIcon::ResizeVertical)
        .on_hover_text(format!(
            "Offset {offset}: {name}0x{value:02x} ({value})\nRight-click to add to the panel.",
            value = *value,
        ))
}
