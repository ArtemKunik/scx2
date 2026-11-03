//! The full effect-parameter editor: all 32 bytes of every effect block.
//!
//! The knobs drawn on the pedals are the fast path; this panel is the complete
//! one. It also shows FUSE's real control names for the model, which is what
//! makes a raw offset like 34 mean `Depth` instead of a mystery byte.

use eframe::egui::{
    self, Align2, CursorIcon, FontId, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2,
};

use super::dial::drag_value;
use super::theme::*;
use super::App;
use crate::models;
use crate::protocol::{self, Kind, BANK_LIVE};

const KINDS: [(Kind, &str); 4] = [
    (Kind::Stomp, "Stomp"),
    (Kind::Mod, "Modulation"),
    (Kind::Delay, "Delay"),
    (Kind::Reverb, "Reverb"),
];

impl App {
    pub(super) fn effect_parameters(&mut self, ui: &mut Ui, preset: &protocol::Preset, width: f32) {
        let openness =
            ui.ctx()
                .animate_bool_with_time(Id::new("fx-params-open"), self.show_fx_params, 0.16);

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
            "Effect Parameters",
            semibold(12.5),
            ink,
        );
        painter.text(
            rect.right_center(),
            Align2::RIGHT_CENTER,
            "All 32 bytes per block · names from FUSE",
            regular(11.5),
            TEXT_3,
        );
        if response.clicked() {
            self.show_fx_params = !self.show_fx_params;
        }
        response.on_hover_cursor(CursorIcon::PointingHand);

        if !self.show_fx_params {
            return;
        }

        ui.add_space(8.0);
        let editable = !self.connected || preset.bank == BANK_LIVE;
        let mut edits = Vec::new();
        for (kind, title) in KINDS {
            let block = preset.block(kind);
            let model = block.and_then(|message| message.model()).unwrap_or(0);
            let before = block
                .map(|message| message.params().to_vec())
                .unwrap_or_else(|| vec![0; 32]);
            let mut params = before.clone();
            let reset = self.effect_card(ui, kind, title, model, &mut params, editable);
            if !reset && params != before {
                edits.push((kind, before, params));
            }
        }
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(
                "The names come from FUSE's model definitions; the SCX2 has not been checked against every model yet. Empty slots show zeroed bytes.",
            )
            .font(regular(11.0))
            .color(TEXT_3),
        );
        ui.add_space(8.0);

        for (kind, before, after) in edits {
            self.apply_effect(kind, &before, &after);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn effect_card(
        &mut self,
        ui: &mut Ui,
        kind: Kind,
        title: &str,
        model: u16,
        params: &mut [u8],
        editable: bool,
    ) -> bool {
        let loaded = model != 0;
        let mut reset = false;
        egui::Frame::none()
            .fill(SIDEBAR)
            .rounding(10.0)
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title).font(semibold(12.5)).color(TEXT));
                    if loaded {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(
                                models::model_name(kind, model).unwrap_or("Unknown"),
                            )
                            .font(regular(12.0))
                            .color(TEXT_2),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("No effect loaded")
                                .font(regular(12.0))
                                .color(TEXT_3),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if loaded && editable && ui.small_button("Reset defaults").clicked() {
                            self.reset_effect(kind);
                            reset = true;
                        }
                    });
                });
                ui.add_space(6.0);

                if !loaded {
                    ui.label(
                        egui::RichText::new("Click the slot on the pedalboard to load one.")
                            .font(regular(11.0))
                            .color(TEXT_3),
                    );
                    return;
                }

                let labels = models::effect_params(kind, model);
                let gap = 5.0;
                let columns = [8usize, 6, 4]
                    .into_iter()
                    .find(|columns| {
                        (ui.available_width() - gap * (*columns as f32 - 1.0)) / *columns as f32
                            >= 48.0
                    })
                    .unwrap_or(4);
                let cell_w = (ui.available_width() - gap * (columns as f32 - 1.0)) / columns as f32;
                ui.spacing_mut().item_spacing = Vec2::splat(gap);
                for start in (0..params.len()).step_by(columns) {
                    ui.horizontal(|ui| {
                        for index in start..(start + columns).min(params.len()) {
                            let label = labels.get(index).copied().unwrap_or("");
                            let used = !label.is_empty();
                            fx_cell(
                                ui,
                                cell_w,
                                protocol::OFF_PARAMS + index,
                                &mut params[index],
                                label,
                                used,
                                editable,
                            );
                        }
                    });
                }
                ui.spacing_mut().item_spacing = Vec2::ZERO;
            });
        ui.add_space(6.0);
        reset
    }
}

#[allow(clippy::too_many_arguments)]
fn fx_cell(
    ui: &mut Ui,
    width: f32,
    offset: usize,
    value: &mut u8,
    label: &'static str,
    used: bool,
    editable: bool,
) -> egui::Response {
    let sense = if editable {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 50.0), sense);
    if editable {
        drag_value(ui, &response, value, 255.0, 1.2);
    }

    let painter = ui.painter();
    let fill = if response.hovered() { RAISED } else { SIDEBAR };
    painter.rect_filled(rect, 6.0, fill);
    painter.text(
        rect.left_top() + Vec2::new(6.0, 4.0),
        Align2::LEFT_TOP,
        offset.to_string(),
        regular(9.5),
        TEXT_3,
    );
    painter.text(
        rect.center() + Vec2::new(0.0, -1.0),
        Align2::CENTER_CENTER,
        format!("{value:02x}"),
        FontId::monospace(13.0),
        if used { TEXT } else { TEXT_3 },
    );
    if used {
        painter.text(
            rect.center_bottom() + Vec2::new(0.0, -5.0),
            Align2::CENTER_BOTTOM,
            truncate(label, 9),
            regular(9.0),
            TEXT_2,
        );
    }

    let track = Rect::from_min_max(
        Pos2::new(rect.left() + 5.0, rect.bottom() - 3.5),
        Pos2::new(rect.right() - 5.0, rect.bottom() - 2.0),
    );
    painter.rect_filled(track, 1.0, HAIRLINE);
    let mut level = track;
    level.set_right(track.left() + track.width() * (*value as f32 / 255.0));
    painter.rect_filled(level, 1.0, if used { TEXT_2 } else { HAIRLINE });

    let name = if used { label } else { "unused" };
    response
        .on_hover_cursor(if editable {
            CursorIcon::ResizeVertical
        } else {
            CursorIcon::Default
        })
        .on_hover_text(format!(
            "Offset {offset}: {name} = 0x{value:02x} ({value})\nEffect names come from FUSE's model map, not SCX2 hardware verification."
        ))
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        text.chars().take(width).collect()
    }
}
