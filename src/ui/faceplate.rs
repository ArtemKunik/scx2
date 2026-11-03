//! The silver faceplate and the effect cards.

use eframe::egui::{Align2, Color32, FontId, Rect, Sense, Ui, Vec2};

use crate::models;
use crate::protocol::{self, Kind};

use super::{stroke, ACCENT, GOOD, JEWEL, MUTED, PANEL, PLATE_INK, SILVER, SILVER_DARK, TEXT};
pub(crate) fn paint_faceplate(ui: &Ui, rect: Rect, voice: &str, connected: bool) {
    let painter = ui.painter();

    // brushed metal: a flat fill, a bright top edge and a dark bottom edge
    painter.rect_filled(rect, 9.0, SILVER);
    let inner = rect.shrink(1.0);
    painter.line_segment(
        [
            inner.left_top() + Vec2::new(6.0, 1.0),
            inner.right_top() + Vec2::new(-6.0, 1.0),
        ],
        stroke(1.0, Color32::from_rgb(236, 239, 244)),
    );
    painter.line_segment(
        [
            inner.left_bottom() + Vec2::new(6.0, -1.0),
            inner.right_bottom() + Vec2::new(-6.0, -1.0),
        ],
        stroke(1.0, SILVER_DARK),
    );
    painter.rect_stroke(rect, 9.0, stroke(1.0, Color32::from_rgb(96, 102, 112)));

    // pilot jewel
    let jewel = rect.left_center() + Vec2::new(24.0, 0.0);
    let glow = if connected {
        JEWEL
    } else {
        Color32::from_rgb(70, 40, 38)
    };
    painter.circle_filled(jewel, 11.0, glow.gamma_multiply(0.25));
    painter.circle_filled(jewel, 6.5, glow);
    painter.circle_filled(
        jewel - Vec2::new(1.5, 1.5),
        2.5,
        glow.gamma_multiply(1.0).linear_multiply(0.8),
    );
    painter.circle_stroke(jewel, 7.5, stroke(1.0, Color32::from_rgb(60, 64, 72)));

    painter.text(
        rect.left_center() + Vec2::new(46.0, -9.0),
        Align2::LEFT_CENTER,
        "SUPER CHAMP X2",
        FontId::proportional(21.0),
        PLATE_INK,
    );
    painter.text(
        rect.left_center() + Vec2::new(48.0, 13.0),
        Align2::LEFT_CENTER,
        "FENDER  -  TUBE AMPLIFIER",
        FontId::proportional(8.5),
        Color32::from_rgb(60, 64, 72),
    );

    painter.text(
        rect.right_center() + Vec2::new(-18.0, -8.0),
        Align2::RIGHT_CENTER,
        voice,
        FontId::proportional(16.0),
        PLATE_INK,
    );
    painter.text(
        rect.right_center() + Vec2::new(-18.0, 12.0),
        Align2::RIGHT_CENTER,
        "VOICE",
        FontId::proportional(8.5),
        Color32::from_rgb(60, 64, 72),
    );
}

pub(crate) fn effect_card(ui: &mut Ui, title: &str, preset: &protocol::Preset, kind: Kind) {
    let width = 214.0;
    let height = 92.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, PANEL);
    painter.rect_stroke(rect, 12.0, stroke(1.0, Color32::from_rgb(40, 44, 53)));

    let model = preset.block(kind).and_then(|m| m.model()).unwrap_or(0);
    let loaded = model != 0;
    painter.text(
        rect.left_top() + Vec2::new(14.0, 12.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(9.5),
        ACCENT,
    );
    let name = if loaded {
        models::model_name(kind, model).unwrap_or("unknown")
    } else {
        "-"
    };
    painter.text(
        rect.left_top() + Vec2::new(14.0, 30.0),
        Align2::LEFT_TOP,
        name,
        FontId::proportional(15.0),
        if loaded { TEXT } else { MUTED },
    );
    if let Some(slot) = preset.block(kind).and_then(|m| m.slot()) {
        painter.text(
            rect.left_bottom() + Vec2::new(14.0, -14.0),
            Align2::LEFT_BOTTOM,
            format!("slot {slot}"),
            FontId::proportional(10.0),
            MUTED,
        );
    }
    if loaded {
        painter.text(
            rect.right_bottom() + Vec2::new(-14.0, -14.0),
            Align2::RIGHT_BOTTOM,
            "on",
            FontId::proportional(10.0),
            GOOD,
        );
    }
}
