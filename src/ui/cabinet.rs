//! The amp itself: cabinet, control panel, pilot jewel and grille.

use eframe::egui::{
    epaint::Shadow, Align2, Color32, Id, Painter, Pos2, Rect, Rounding, Sense, Shape, Ui, Vec2,
};

use super::ampstyle::{self, Look};
use super::app::Knob;
use super::art::{Art, JEWEL_D};
use super::dial::{can_add, knob_widget, remove_button, Setting, DIAL_R};
use super::knobs::remove_knob;
use super::paint::{full_uv, paint_grille, screw, snapped, spaced, tile, vertical_gradient};
use super::theme::*;
use super::App;
use crate::models;
use crate::protocol::{self, Kind, BANK_LIVE};

pub(super) const CABINET_PAD: f32 = 16.0;

const SLOT_W: f32 = 102.0;

const SLOT_H: f32 = 138.0;

const PILOT_W: f32 = 84.0;

const PANEL_PAD_Y: f32 = 18.0;

/// Knobs per panel row and the resulting panel height.
pub(super) fn panel_layout(cabinet_width: f32, knobs: &[Knob]) -> (usize, f32) {
    let usable = cabinet_width - 2.0 * CABINET_PAD - PILOT_W - 16.0;
    let columns = ((usable / SLOT_W).floor() as usize).max(1);
    let rows = knobs.len().max(1).div_ceil(columns);
    (columns, rows as f32 * SLOT_H + 2.0 * PANEL_PAD_Y - 12.0)
}

impl App {
    pub(super) fn amp(
        &mut self,
        ui: &mut Ui,
        art: &Art,
        preset: &protocol::Preset,
        width: f32,
        panel_h: f32,
        grille_h: f32,
    ) {
        let Some(block) = preset.block(Kind::Amp) else {
            return;
        };
        let mut frame = *block.as_bytes();
        let before = frame;
        let baseline = self.baseline(preset);
        // A stored slot cannot be written yet, so only the live buffer is editable.
        let editable = !self.connected || preset.bank == BANK_LIVE;

        // The cabinet follows the voice: a '57 Champ is tweed, a '65 Twin is
        // blackface, and a British voice gets its salt-and-pepper grille.
        let model = block
            .model()
            .filter(|model| *model != 0)
            .unwrap_or_else(|| models::voice_model_id(frame[44]));
        let look = ampstyle::for_model(model);
        let surface = art.surface(look.era);
        let name = models::model_name(Kind::Amp, model).unwrap_or("Super Champ X2");

        let height = CABINET_PAD + panel_h + 14.0 + grille_h + CABINET_PAD;
        let (cabinet, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter().clone();

        // cabinet
        painter.add(Shape::mesh(
            Shadow {
                offset: Vec2::new(0.0, 10.0),
                blur: 26.0,
                spread: 0.0,
                color: Color32::from_black_alpha(150),
            }
            .tessellate(cabinet, Rounding::same(16.0)),
        ));
        tile(
            &painter,
            cabinet,
            16.0,
            &surface.tolex,
            art.ppp,
            Color32::WHITE,
        );
        painter.rect_stroke(cabinet, 16.0, stroke(1.0, Color32::from_white_alpha(14)));

        // control panel in a thin chrome trim
        let panel = Rect::from_min_size(
            cabinet.min + Vec2::splat(CABINET_PAD),
            Vec2::new(width - 2.0 * CABINET_PAD, panel_h),
        );
        painter.rect_filled(panel.expand(1.5), 6.0, look.piping.gamma_multiply(0.62));
        painter.line_segment(
            [
                panel.left_top() + Vec2::new(4.0, -1.0),
                panel.right_top() + Vec2::new(-4.0, -1.0),
            ],
            stroke(1.0, look.piping),
        );
        tile(
            &painter,
            panel,
            5.0,
            &surface.panel,
            art.ppp,
            Color32::WHITE,
        );
        let sheen = Rect::from_min_size(panel.min, Vec2::new(panel.width(), 44.0))
            .shrink2(Vec2::new(1.0, 0.0));
        vertical_gradient(
            &painter,
            sheen,
            Color32::from_white_alpha(10),
            Color32::TRANSPARENT,
        );
        for corner in [
            panel.left_top() + Vec2::new(11.0, 11.0),
            panel.right_top() + Vec2::new(-11.0, 11.0),
            panel.left_bottom() + Vec2::new(11.0, -11.0),
            panel.right_bottom() + Vec2::new(-11.0, -11.0),
        ] {
            screw(&painter, corner);
        }

        self.pilot(ui, &painter, art, &look, panel);

        // knobs
        let (columns, _) = panel_layout(width, &self.knobs);
        let origin = panel.min + Vec2::new(PILOT_W, PANEL_PAD_Y);
        // Knobs share the panel evenly, the way they do on the amp.
        let used = columns.min(self.knobs.len().max(1));
        let spread = (panel.width() - PILOT_W - 28.0) / used as f32;
        let slot_at = |index: usize| {
            let (row, column) = (index / columns, index % columns);
            Rect::from_min_size(
                origin + Vec2::new(column as f32 * spread, row as f32 * SLOT_H),
                Vec2::new(spread, SLOT_H),
            )
        };

        let mut remove = None;
        let knobs = self.knobs.clone();
        let mut placed = 0;
        for knob in &knobs {
            if knob.offset >= protocol::REPORT_SIZE {
                continue;
            }
            let slot = slot_at(placed);
            placed += 1;
            let response = knob_widget(
                ui,
                &painter,
                art,
                &look,
                slot,
                knob,
                Setting {
                    value: &mut frame[knob.offset],
                    baseline: baseline[knob.offset],
                    editable,
                },
            );
            if response.dragged() {
                self.active_offset = Some(knob.offset);
            }
            response.context_menu(|ui| {
                if ui.button("Remove from Panel").clicked() {
                    remove = Some(knob.offset);
                    ui.close_menu();
                }
            });
            if ui.rect_contains_pointer(slot)
                && remove_button(ui, &painter, slot, knob.offset, look.ink)
            {
                remove = Some(knob.offset);
            }
        }
        if let Some(offset) = remove {
            remove_knob(&mut self.knobs, offset);
        }
        if can_add(&self.knobs) {
            self.add_button(ui, &painter, panel, look.ink);
        }

        // grille cloth
        let grille = Rect::from_min_size(
            Pos2::new(panel.left(), panel.bottom() + 14.0),
            Vec2::new(panel.width(), grille_h),
        );
        paint_grille(&painter, art, &look, grille, name);

        self.commit(&frame, &before);
    }

    /// Apply whatever the widgets changed in this frame's copy of the block.
    pub(super) fn commit(
        &mut self,
        frame: &[u8; protocol::REPORT_SIZE],
        before: &[u8; protocol::REPORT_SIZE],
    ) {
        for (offset, value) in frame.iter().enumerate() {
            if *value != before[offset] {
                self.apply_edit(Kind::Amp, offset, *value);
            }
        }
    }

    /// The amp block as last read from the amp, which edits are compared to.
    pub(super) fn baseline(&self, preset: &protocol::Preset) -> [u8; protocol::REPORT_SIZE] {
        self.original
            .get(preset.bank, preset.index)
            .and_then(|known| known.block(Kind::Amp))
            .or(preset.block(Kind::Amp))
            .map(|block| *block.as_bytes())
            .unwrap_or([0; protocol::REPORT_SIZE])
    }

    /// The jewel is the connection indicator, exactly as it is on the amp:
    /// lit means alive.
    fn pilot(&self, ui: &mut Ui, painter: &Painter, art: &Art, look: &Look, panel: Rect) {
        let center = Pos2::new(
            panel.left() + PILOT_W / 2.0 + 4.0,
            panel.top() + PANEL_PAD_Y + 20.0 + DIAL_R,
        );
        let level = if self.connected {
            1.0
        } else if self.connecting {
            let time = ui.input(|i| i.time) as f32;
            0.35 + 0.35 * (time * 3.5).sin().abs()
        } else {
            0.0
        };

        spaced(
            painter,
            Pos2::new(center.x, panel.top() + PANEL_PAD_Y + 2.0),
            Align2::CENTER_TOP,
            "POWER",
            semibold(10.5),
            look.ink,
            1.4,
        );

        let shade = (70.0 + level * 185.0) as u8;
        let rect = snapped(
            Rect::from_center_size(center, Vec2::splat(JEWEL_D)),
            art.ppp,
        );
        painter.image(
            art.glow.id(),
            rect.expand(5.0),
            full_uv(),
            Color32::from_black_alpha(190),
        );
        painter.image(
            art.jewel(look.jewel).id(),
            rect,
            full_uv(),
            Color32::from_gray(shade),
        );
        if level > 0.0 {
            painter.image(
                art.glow.id(),
                rect.expand(26.0),
                full_uv(),
                look.accent.gamma_multiply(level * 0.45),
            );
        }

        let tip = if self.connected {
            "Connected to the amp"
        } else if self.connecting {
            "Connecting to the amp"
        } else {
            "Amp not connected"
        };
        ui.interact(rect.expand(6.0), Id::new("pilot"), Sense::hover())
            .on_hover_text(tip);
    }
}
