//! The `+ knob` menu: front-panel controls first, guessed hidden ones after.

use eframe::egui::{self, Color32, FontId, Ui};

use crate::params;

use super::app::Knob;
use super::{ACCENT, MUTED};

/// Menu of knobs that can be put on the amp card. The front-panel knob set
/// comes first; the rest are parameters only direct control can reach, with
/// our best guess at what they do.
pub(crate) fn knob_menu(ui: &mut Ui, knobs: &mut Vec<Knob>) {
    ui.menu_button("+ knob", |ui| {
        ui.set_min_width(260.0);
        heading(ui, "FRONT PANEL", ACCENT);
        knob_buttons(ui, knobs, true);
        ui.add_space(4.0);
        ui.separator();
        heading(ui, "HIDDEN - GUESSED", MUTED);
        knob_buttons(ui, knobs, false);
        if knobs.len() >= 16 {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("knob limit reached (16)")
                    .font(FontId::proportional(10.0))
                    .color(MUTED),
            );
        }
    });
}

fn heading(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(
        egui::RichText::new(text)
            .font(FontId::proportional(9.0))
            .color(color)
            .strong(),
    );
}

fn knob_buttons(ui: &mut Ui, knobs: &mut Vec<Knob>, standard: bool) {
    for spec in params::KNOBS
        .iter()
        .filter(|spec| spec.standard == standard)
    {
        let added = knobs.iter().any(|knob| knob.offset == spec.offset);
        let text = if added {
            format!("{}  (added)", spec.name)
        } else {
            spec.name.to_string()
        };
        if ui
            .add_enabled(
                !added && knobs.len() < 16,
                egui::Button::new(text).min_size(egui::Vec2::new(230.0, 0.0)),
            )
            .on_hover_text(spec.hint)
            .clicked()
        {
            knobs.push(Knob {
                offset: spec.offset,
                label: spec.name.to_string(),
            });
            ui.close_menu();
        }
    }
}
