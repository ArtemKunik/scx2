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
        if knobs.len() >= MAX_KNOBS {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("knob limit reached")
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
                !added && knobs.len() < MAX_KNOBS,
                egui::Button::new(text).min_size(egui::Vec2::new(230.0, 0.0)),
            )
            .on_hover_text(spec.hint)
            .clicked()
        {
            add_knob(knobs, spec.offset);
            ui.close_menu();
        }
    }
}

/// Knobs are capped so the card stays readable; every one can still be
/// removed again.
pub(crate) const MAX_KNOBS: usize = 16;

/// Bind a block offset to a knob. Returns false if it is already bound.
pub(crate) fn add_knob(knobs: &mut Vec<Knob>, offset: usize) -> bool {
    if knobs.len() >= MAX_KNOBS || knobs.iter().any(|knob| knob.offset == offset) {
        return false;
    }
    knobs.push(Knob {
        offset,
        label: params::offset_title(offset).unwrap_or_else(|| format!("P{offset}")),
    });
    true
}

/// Unbind the knob for an offset. Returns whether one was removed.
pub(crate) fn remove_knob(knobs: &mut Vec<Knob>, offset: usize) -> bool {
    let before = knobs.len();
    knobs.retain(|knob| knob.offset != offset);
    knobs.len() != before
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol;

    #[test]
    fn a_knob_can_be_added_and_removed_again() {
        let mut knobs = Vec::new();
        assert!(add_knob(&mut knobs, 32));
        assert!(add_knob(&mut knobs, 39));
        assert_eq!(knobs.len(), 2);
        assert_eq!(knobs[0].label, "Volume");

        assert!(!add_knob(&mut knobs, 39), "duplicates are rejected");
        assert!(remove_knob(&mut knobs, 39));
        assert_eq!(knobs.len(), 1);
        assert_eq!(knobs[0].offset, 32);
        assert!(!remove_knob(&mut knobs, 39), "removing twice is harmless");
    }

    #[test]
    fn the_knob_limit_is_enforced() {
        let mut knobs = Vec::new();
        for offset in protocol::OFF_PARAMS..protocol::OFF_PARAMS + MAX_KNOBS {
            assert!(add_knob(&mut knobs, offset));
        }
        assert!(!add_knob(&mut knobs, protocol::REPORT_SIZE - 1));
        assert_eq!(knobs.len(), MAX_KNOBS);
        assert!(remove_knob(&mut knobs, protocol::OFF_PARAMS));
        assert!(add_knob(&mut knobs, protocol::REPORT_SIZE - 1));
    }
}
