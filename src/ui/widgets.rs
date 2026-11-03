//! Small painted widgets: skirted knobs, parameter cells, status pills.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Ui, Vec2};

use crate::models;
use crate::params;
use crate::protocol::Kind;

use super::{stroke, ACCENT, EDITED, HOVER, MUTED, PANEL_HI, TEXT};
pub(crate) fn knob_value_label(offset: usize, value: u8) -> String {
    if (44..=46).contains(&offset) {
        models::model_name(Kind::Amp, voice_model_id(value))
            .unwrap_or("?")
            .to_string()
    } else {
        format!("0x{value:02x}  {value}")
    }
}

/// Knobs that select a voice walk 0..12 instead of the full byte range.
pub(crate) fn knob_range(offset: usize) -> f32 {
    if (44..=46).contains(&offset) {
        13.0
    } else {
        255.0
    }
}

pub(crate) fn voice_model_id(index: u8) -> u16 {
    match index {
        0x00 => 0x00eb,
        0x01 => 0x0067,
        0x02 => 0x0064,
        0x03 => 0x0053,
        0x04 => 0x006a,
        0x05 => 0x0075,
        0x06 => 0x0072,
        0x07 => 0x0061,
        0x08 => 0x006d,
        0x09 => 0x005e,
        0x0a => 0x005d,
        0x0b => 0x0079,
        0x0c => 0x007c,
        0x0d => 0x00ea,
        _ => 0,
    }
}

pub(crate) fn knob_widget(
    ui: &mut Ui,
    value: &mut u8,
    label: &str,
    sub: &str,
    range: f32,
    accent: Color32,
) -> (egui::Response, Rect) {
    let size = Vec2::new(92.0, 108.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if response.dragged() {
        let pixels = -response.drag_delta().y + response.drag_delta().x;
        let delta = pixels * (range / 160.0);
        let next = (*value as f32 + delta).clamp(0.0, range).round() as u8;
        if next != *value {
            *value = next;
            response.changed = true;
        }
    }

    let center = rect.center_top() + Vec2::new(0.0, 50.0);
    let radius = 27.0;
    let start = 135f32.to_radians();
    let end = 405f32.to_radians();
    let t = (*value as f32 / range).clamp(0.0, 1.0);
    let painter = ui.painter();

    painter.circle_filled(
        center + Vec2::new(0.0, 3.0),
        radius + 7.0,
        Color32::from_rgb(6, 7, 9),
    );
    painter.circle_filled(center, radius + 6.0, Color32::from_rgb(16, 17, 21));
    painter.circle_stroke(
        center,
        radius + 6.0,
        stroke(1.0, Color32::from_rgb(48, 51, 60)),
    );
    for tick in 0..11 {
        let angle = start + (end - start) * tick as f32 / 10.0;
        let dir = Vec2::new(angle.cos(), angle.sin());
        painter.line_segment(
            [center + dir * (radius + 2.5), center + dir * (radius + 5.0)],
            stroke(1.0, Color32::from_rgb(74, 79, 90)),
        );
    }

    let track = arc_points(center, radius + 10.0, start, end);
    painter.add(egui::Shape::line(
        track,
        stroke(4.0, Color32::from_rgb(46, 50, 60)),
    ));
    if t > 0.005 {
        let arc = arc_points(center, radius + 10.0, start, start + (end - start) * t);
        painter.add(egui::Shape::line(arc, stroke(4.0, accent)));
    }

    // knob body: a few concentric circles fake a soft bevel
    painter.circle_filled(center, radius, Color32::from_rgb(22, 24, 29));
    painter.circle_filled(
        center - Vec2::new(0.0, 1.0),
        radius - 2.0,
        Color32::from_rgb(36, 39, 47),
    );
    painter.circle_filled(
        center - Vec2::new(0.0, 3.0),
        radius - 7.0,
        Color32::from_rgb(46, 50, 60),
    );
    painter.circle_stroke(center, radius, stroke(1.0, Color32::from_rgb(58, 63, 74)));

    let angle = start + (end - start) * t;
    let dir = Vec2::new(angle.cos(), angle.sin());
    painter.line_segment(
        [
            center + dir * (radius * 0.3),
            center + dir * (radius * 0.82),
        ],
        stroke(3.0, Color32::WHITE),
    );

    if response.hovered() {
        painter.circle_stroke(
            center,
            radius + 14.0,
            stroke(1.0, accent.gamma_multiply(0.5)),
        );
    }

    painter.text(
        rect.center_top() + Vec2::new(0.0, 2.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(11.0),
        MUTED,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 88.0),
        Align2::CENTER_TOP,
        sub,
        FontId::proportional(10.5),
        if response.hovered() { TEXT } else { MUTED },
    );

    ui.advance_cursor_after_rect(rect);
    (response.on_hover_text(format!("{label} - {sub}")), rect)
}

pub(crate) fn arc_points(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = 48;
    (0..=steps)
        .map(|i| {
            let angle = from + (to - from) * i as f32 / steps as f32;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

pub(crate) fn param_cell(
    ui: &mut Ui,
    offset: usize,
    value: &mut u8,
    original: u8,
    flash: f32,
) -> egui::Response {
    let size = Vec2::new(52.0, 60.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if response.dragged() {
        let delta = (-response.drag_delta().y + response.drag_delta().x) * 1.2;
        let next = (*value as f32 + delta).clamp(0.0, 255.0).round() as u8;
        if next != *value {
            *value = next;
            response.changed = true;
        }
    }

    let painter = ui.painter();
    let edited = *value != original;
    let bg = if edited {
        Color32::from_rgb(28, 38, 52)
    } else if response.hovered() {
        HOVER
    } else {
        PANEL_HI
    };
    painter.rect_filled(rect, 7.0, bg);
    if edited {
        painter.rect_stroke(rect, 7.0, stroke(1.0, EDITED));
    }

    let fill_height = rect.height() * (*value as f32 / 255.0);
    let fill = Rect::from_min_max(
        Pos2::new(rect.left() + 4.0, rect.bottom() - fill_height),
        Pos2::new(rect.right() - 4.0, rect.bottom() - 4.0),
    );
    if fill.height() > 1.0 {
        painter.rect_filled(fill, 4.0, ACCENT.gamma_multiply(0.22));
    }

    if flash > 0.0 {
        let alpha = (flash / 1.4).clamp(0.0, 1.0);
        painter.rect_stroke(rect, 7.0, stroke(2.0, ACCENT.gamma_multiply(alpha)));
    }

    painter.text(
        rect.center_top() + Vec2::new(0.0, 5.0),
        Align2::CENTER_TOP,
        format!("{offset}"),
        FontId::proportional(9.0),
        MUTED,
    );
    painter.text(
        rect.center_top() + Vec2::new(0.0, 19.0),
        Align2::CENTER_TOP,
        format!("{:02x}", value),
        FontId::monospace(13.0),
        if edited { EDITED } else { TEXT },
    );
    let short = params::short_offset_label(offset);
    if !short.is_empty() {
        painter.text(
            rect.center_top() + Vec2::new(0.0, 40.0),
            Align2::CENTER_TOP,
            short,
            FontId::proportional(8.5),
            if params::offset_label(offset).is_some() {
                Color32::from_rgb(150, 156, 168)
            } else {
                MUTED
            },
        );
    }

    ui.advance_cursor_after_rect(rect);
    let name = params::offset_label(offset)
        .map(|label| format!("{label} - "))
        .unwrap_or_default();
    response.on_hover_text(format!(
        "offset {offset} - {name}0x{value:02x} ({value}) - right-click to bind a knob"
    ))
}
