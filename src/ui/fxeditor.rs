//! The focused effect editor: one pedal at a time, with controls that are
//! large enough to read and hit.
//!
//! The pedalboard is an overview. Clicking a slot opens this card, which keeps
//! the pedal drawing as the identity but moves every working control into a
//! dedicated rack: a big knob per parameter, its name above and its value
//! below. Nothing is hidden behind a drag target the size of a peppercorn.

use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Margin, Pos2, Rect, Sense, Ui, Vec2};

use super::art::Art;
use super::dial::{drag_value, scale10};
use super::hardware::{knob_angle, small_knob, KNOB_START, KNOB_SWEEP};
use super::paint::spaced;
use super::pedalstage;
use super::theme::*;
use super::{fx, App};
use crate::models;
use crate::protocol::{self, Kind, BANK_LIVE};

const CARD_MAX: Vec2 = Vec2::new(1020.0, 640.0);

impl App {
    pub(super) fn effect_editor(&mut self, ctx: &egui::Context, art: &Art) {
        let Some(kind) = self.fx_editor else {
            return;
        };
        let Some(preset) = self.dump.get(self.selected.0, self.selected.1).cloned() else {
            self.fx_editor = None;
            return;
        };
        let editable = !self.connected || preset.bank == BANK_LIVE;
        let block = preset.block(kind);
        let model = block.and_then(|message| message.model()).unwrap_or(0);
        let loaded = model != 0;
        let before = block
            .map(|message| *message.as_bytes())
            .unwrap_or([0; protocol::REPORT_SIZE]);
        let mut frame = before;
        let baseline = self.effect_baseline(&preset, kind);
        let was_bypassed = block.is_some_and(|message| message.byte(protocol::OFF_BYPASS) != 0);
        let mut bypassed = was_bypassed;
        let look = fx::look(kind, model).filter(|_| loaded);
        let title = kind.label().to_uppercase();
        let name = if loaded {
            models::model_name(kind, model).unwrap_or("Unknown effect")
        } else {
            "No effect loaded"
        };
        let count = models::effect_knobs(kind, model);
        let labels = models::effect_params(kind, model);
        let defaults = models::default_params(kind, model);

        let screen = ctx.screen_rect();
        let card = Rect::from_center_size(
            screen.center(),
            Vec2::new(
                (screen.width() - 120.0).clamp(680.0, CARD_MAX.x),
                (screen.height() - 120.0).clamp(500.0, CARD_MAX.y),
            ),
        );

        // A shade over the editor, and the click that dismisses it.
        let dismiss = egui::Area::new(Id::new("fx-editor-shade"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, Color32::from_black_alpha(150));
                response.clicked()
            })
            .inner;
        if (dismiss && self.fx_editor_armed) || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.fx_editor = None;
            return;
        }
        // From the next frame on, a click on the shade is a real dismissal.
        self.fx_editor_armed = true;

        let mut close = false;
        let mut reset = false;
        let mut remove = false;
        let mut chosen: Option<u16> = None;

        egui::Area::new(Id::new("fx-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(card.min)
            .show(ctx, |ui| {
                // Claim the whole card, including its empty margins, so a
                // click inside it cannot fall through to the dismissing shade.
                let _ = ui.interact(
                    Rect::from_min_size(card.min, card.size()),
                    Id::new("fx-editor-card"),
                    Sense::click_and_drag(),
                );
                egui::Frame::none()
                    .fill(Color32::from_rgb(25, 24, 22))
                    .stroke(stroke(1.0, HAIRLINE))
                    .rounding(14.0)
                    .inner_margin(Margin::same(22.0))
                    .show(ui, |ui| {
                        ui.set_width(card.width() - 44.0);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;

                        // ---- header --------------------------------------
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                spaced(
                                    ui.painter(),
                                    ui.available_rect_before_wrap().left_top(),
                                    Align2::LEFT_TOP,
                                    &title,
                                    semibold(10.0),
                                    TEXT_3,
                                    1.8,
                                );
                                ui.add_space(16.0);
                                ui.label(
                                    egui::RichText::new(name).font(semibold(21.0)).color(TEXT),
                                );
                                ui.add_space(3.0);
                                let state = if !loaded {
                                    "Pick a model to put an effect in this slot."
                                } else if bypassed {
                                    "Bypassed — the model and its settings are kept."
                                } else if editable {
                                    "Turn the controls below; the amp hears each change."
                                } else {
                                    "Stored preset — choose the Live Buffer to edit."
                                };
                                ui.label(
                                    egui::RichText::new(state).font(regular(11.5)).color(TEXT_3),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if loaded && power_pill(ui, bypassed, editable) {
                                    bypassed = !bypassed;
                                }
                            });
                        });

                        ui.add_space(14.0);
                        divider(ui, 1.0);
                        ui.add_space(14.0);

                        // ---- body ----------------------------------------
                        let content_w = card.width() - 44.0;
                        let art_w = (content_w * 0.34).clamp(260.0, 340.0);
                        // A wide tape or rack unit needs height for its
                        // hardware, not for empty wall above and below it.
                        let stage_h = match look.as_ref().map(|look| look.form) {
                            Some(fx::Form::Box) => 402.0,
                            Some(fx::Form::Wah) => 340.0,
                            Some(_) => 300.0,
                            None => 402.0,
                        };
                        ui.horizontal_top(|ui| {
                            let (stage, _) =
                                ui.allocate_exact_size(Vec2::new(art_w, stage_h), Sense::hover());
                            stage_panel(ui.painter(), art, stage, look.as_ref(), &frame, loaded);

                            ui.add_space(24.0);
                            ui.vertical(|ui| {
                                let controls_w = ui.available_width();
                                if self.fx_editor_list || !loaded {
                                    ui.horizontal(|ui| {
                                        spaced(
                                            ui.painter(),
                                            ui.available_rect_before_wrap().left_top(),
                                            Align2::LEFT_TOP,
                                            "CHOOSE AN EFFECT",
                                            semibold(10.0),
                                            TEXT_3,
                                            1.6,
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Min),
                                            |ui| {
                                                if loaded
                                                    && ui.small_button("Back to controls").clicked()
                                                {
                                                    self.fx_editor_list = false;
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(12.0);
                                    ui.label(
                                        egui::RichText::new("SCX2 = seen in the bundled capture")
                                            .font(regular(9.5))
                                            .color(TEXT_3),
                                    );
                                    ui.add_space(6.0);
                                    chosen = model_list(ui, kind, model, editable);
                                } else {
                                    ui.horizontal(|ui| {
                                        spaced(
                                            ui.painter(),
                                            ui.available_rect_before_wrap().left_top(),
                                            Align2::LEFT_TOP,
                                            "CONTROLS",
                                            semibold(10.0),
                                            TEXT_3,
                                            1.6,
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Min),
                                            |ui| {
                                                if editable
                                                    && ui.small_button("Change effect…").clicked()
                                                {
                                                    self.fx_editor_list = true;
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(10.0);
                                    knob_rack(
                                        ui,
                                        controls_w,
                                        labels,
                                        count,
                                        &mut frame,
                                        &baseline,
                                        defaults.as_ref(),
                                        editable,
                                        look.as_ref(),
                                    );
                                    if count == 0 {
                                        ui.label(
                                            egui::RichText::new(
                                                "This model does not expose named controls. \
                                                 Open Effect Parameters for its raw bytes.",
                                            )
                                            .font(regular(11.5))
                                            .color(TEXT_3),
                                        );
                                    }
                                }
                            });
                        });

                        ui.add_space(12.0);
                        divider(ui, 1.0);
                        ui.add_space(10.0);

                        // ---- footer --------------------------------------
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    "Drag a control or scroll over it · Shift = fine · \
                                     Double-click resets one control",
                                )
                                .font(regular(10.5))
                                .color(TEXT_3),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button(egui::RichText::new("Done").font(semibold(11.0)))
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                    if loaded && editable && ui.button("Reset all").clicked() {
                                        reset = true;
                                    }
                                    if editable && loaded && ui.button("Remove").clicked() {
                                        remove = true;
                                    }
                                },
                            );
                        });
                    });
            });

        if let Some(id) = chosen {
            self.set_effect_model(kind, id);
            self.fx_editor_list = false;
            self.fx_editor = (id != 0).then_some(kind);
            return;
        }
        if reset {
            self.reset_effect(kind);
        }
        if remove {
            self.set_effect_model(kind, 0);
            self.fx_editor = None;
            return;
        }
        if bypassed != was_bypassed {
            self.apply_edit(kind, protocol::OFF_BYPASS, u8::from(bypassed));
        }
        if frame[protocol::OFF_PARAMS..] != before[protocol::OFF_PARAMS..] {
            self.apply_effect(
                kind,
                &before[protocol::OFF_PARAMS..],
                &frame[protocol::OFF_PARAMS..],
            );
        }
        if close {
            self.fx_editor = None;
        }
    }
}

fn stage_panel(
    painter: &egui::Painter,
    art: &Art,
    rect: Rect,
    look: Option<&fx::Pedal>,
    frame: &[u8; protocol::REPORT_SIZE],
    loaded: bool,
) {
    painter.rect_filled(rect, 12.0, Color32::from_rgb(17, 16, 15));
    painter.rect_stroke(rect, 12.0, stroke(1.0, HAIRLINE));
    let stage = rect.shrink2(Vec2::new(8.0, 12.0));
    match look {
        Some(look) => {
            pedalstage::paint(painter, art, stage, look, &frame[protocol::OFF_PARAMS..]);
        }
        None => {
            let center = stage.center();
            painter.circle_stroke(center, 30.0, stroke(1.2, Color32::from_white_alpha(30)));
            painter.line_segment(
                [center - Vec2::new(11.0, 0.0), center + Vec2::new(11.0, 0.0)],
                stroke(1.4, TEXT_3),
            );
            painter.line_segment(
                [center - Vec2::new(0.0, 11.0), center + Vec2::new(0.0, 11.0)],
                stroke(1.4, TEXT_3),
            );
            spaced(
                painter,
                center + Vec2::new(0.0, 48.0),
                Align2::CENTER_TOP,
                if loaded { "NO PICTURE" } else { "EMPTY SLOT" },
                semibold(9.0),
                TEXT_3,
                1.6,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn knob_rack(
    ui: &mut Ui,
    width: f32,
    labels: &'static [&'static str],
    count: usize,
    frame: &mut [u8; protocol::REPORT_SIZE],
    baseline: &[u8; protocol::REPORT_SIZE],
    defaults: Option<&[u8; 32]>,
    editable: bool,
    look: Option<&fx::Pedal>,
) {
    let gap = 10.0;
    let columns = if width >= 500.0 { 3 } else { 2 };
    let cell_w = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    let knob = look
        .map(|look| look.knob)
        .unwrap_or(Color32::from_rgb(28, 28, 30));
    let cap = look
        .map(|look| look.knob_cap)
        .unwrap_or(Color32::from_rgb(176, 178, 180));

    for start in (0..count).step_by(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // A short last row is centred under the full rows, the way the
            // knobs on the pedal are.
            let used = (start + columns).min(count) - start;
            if used < columns {
                ui.add_space((columns - used) as f32 * (cell_w + gap) / 2.0);
            }
            for index in start..(start + columns).min(count) {
                let offset = protocol::OFF_PARAMS + index;
                let label = labels.get(index).copied().unwrap_or("PARAM");
                let default = defaults.map(|values| values[index]).unwrap_or(0x80);
                let edited = frame[offset] != baseline[offset];
                big_knob(
                    ui,
                    cell_w,
                    label,
                    &mut frame[offset],
                    editable,
                    edited,
                    default,
                    knob,
                    cap,
                );
            }
        });
        ui.add_space(4.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn big_knob(
    ui: &mut Ui,
    width: f32,
    label: &str,
    value: &mut u8,
    editable: bool,
    edited: bool,
    default: u8,
    knob: Color32,
    cap: Color32,
) {
    let sense = if editable {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 146.0), sense);
    if editable {
        drag_value(ui, &response, value, 255.0, 1.1);
        if response.double_clicked() {
            *value = default;
        }
        // A knob is turned, not tabbed to. Without this egui leaves a dashed
        // keyboard-focus box around the whole cell after a click.
        if response.clicked() {
            response.surrender_focus();
        }
    }
    let response = response
        .on_hover_cursor(if editable {
            CursorIcon::ResizeVertical
        } else {
            CursorIcon::Default
        })
        .on_hover_text(if editable {
            format!(
                "{label}: {} / 10\nDrag or scroll · Shift fine · Double-click resets",
                scale10(*value)
            )
        } else {
            format!(
                "{label}: {} / 10\nChoose the Live Buffer to edit effects.",
                scale10(*value)
            )
        });

    let painter = ui.painter();
    let center = Pos2::new(rect.center().x, rect.top() + 52.0);
    let radius = 28.0;

    spaced(
        painter,
        Pos2::new(rect.center().x, rect.top() + 12.0),
        Align2::CENTER_TOP,
        &label.to_uppercase(),
        semibold(11.0),
        if response.hovered() { TEXT } else { TEXT_2 },
        1.2,
    );

    // Tick marks give the knob a scale even when it is not being dragged.
    for tick in 0..=10 {
        let t = tick as f32 / 10.0;
        let angle = KNOB_START + t * KNOB_SWEEP;
        let dir = Vec2::new(angle.cos(), angle.sin());
        painter.line_segment(
            [
                center + dir * (radius + 5.0),
                center + dir * (radius + if tick % 5 == 0 { 9.5 } else { 7.5 }),
            ],
            stroke(
                if tick % 5 == 0 { 1.0 } else { 0.7 },
                Color32::from_white_alpha(if response.hovered() { 70 } else { 42 }),
            ),
        );
    }

    small_knob(painter, center, radius, knob, cap, knob_angle(*value));
    if edited {
        painter.circle_stroke(
            center,
            radius + 3.0,
            stroke(1.4, AMBER.gamma_multiply(0.75)),
        );
    }
    if response.hovered() || response.dragged() {
        painter.circle_stroke(
            center,
            radius + (if edited { 5.0 } else { 3.0 }),
            stroke(1.2, Color32::from_white_alpha(130)),
        );
    }

    painter.text(
        Pos2::new(rect.center().x, center.y + radius + 12.0),
        Align2::CENTER_TOP,
        scale10(*value),
        if edited {
            semibold(15.0)
        } else {
            regular(15.0)
        },
        if edited { AMBER } else { TEXT },
    );
    painter.text(
        Pos2::new(rect.center().x, center.y + radius + 33.0),
        Align2::CENTER_TOP,
        "0–10",
        regular(8.5),
        TEXT_3,
    );
}

fn model_list(ui: &mut Ui, kind: Kind, current: u16, editable: bool) -> Option<u16> {
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (id, name) in models::table(kind).unwrap_or(&[]) {
                let text = if models::observed(kind, *id) {
                    format!("{name}    SCX2")
                } else {
                    name.to_string()
                };
                let row = ui.add_sized(
                    [ui.available_width(), 26.0],
                    egui::SelectableLabel::new(*id == current, text),
                );
                if row.clicked() && editable {
                    chosen = Some(*id);
                }
            }
        });
    chosen
}

fn divider(ui: &mut Ui, thickness: f32) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), thickness), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, HAIRLINE);
}

fn power_pill(ui: &mut Ui, bypassed: bool, interactive: bool) -> bool {
    let sense = if interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(82.0, 30.0), sense);
    let (fill, hover, ink) = if bypassed {
        (
            Color32::from_white_alpha(10),
            Color32::from_white_alpha(20),
            TEXT_3,
        )
    } else {
        (
            AMBER.gamma_multiply(0.16),
            AMBER.gamma_multiply(0.26),
            AMBER,
        )
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 15.0, if response.hovered() { hover } else { fill });
    painter.rect_stroke(rect, 15.0, stroke(1.0, ink.gamma_multiply(0.5)));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        if bypassed { "BYPASSED" } else { "ON" },
        semibold(9.0),
        ink,
    );
    if interactive {
        let response = response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text(if bypassed {
                "Switch this effect on. Its settings are kept."
            } else {
                "Switch this effect off. Its settings are kept."
            });
        return response.clicked();
    }
    false
}
