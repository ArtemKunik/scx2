//! Which knobs are on the panel: adding, removing and the limit.

use eframe::egui::{self, Color32, CursorIcon, Id, Painter, Rect, Sense, Ui, Vec2};

use super::app::Knob;
use super::theme::semibold;
use super::App;
use crate::params;

/// Knobs are capped so the panel stays readable; every one can still be
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

impl App {
    /// The `+` on the panel, with a menu of every knob that is not shown yet.
    pub(super) fn add_button(&mut self, ui: &mut Ui, painter: &Painter, panel: Rect, ink: Color32) {
        let center = panel.right_top() + Vec2::new(-34.0, 26.0);
        let rect = Rect::from_center_size(center, Vec2::splat(30.0));
        let popup = Id::new("add-knob-menu");
        let open = ui.memory(|memory| memory.is_popup_open(popup));
        let mut response = ui
            .interact(rect, Id::new("add-knob"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if !open {
            response = response.on_hover_text("Add a knob to the panel");
        }

        // Stays out of the way until the pointer is on the panel.
        if open || ui.rect_contains_pointer(panel) {
            let ink = if response.hovered() || open {
                ink
            } else {
                ink.gamma_multiply(0.65)
            };
            painter.circle_stroke(center, 11.0, super::theme::stroke(1.0, ink));
            painter.line_segment(
                [center - Vec2::new(4.5, 0.0), center + Vec2::new(4.5, 0.0)],
                super::theme::stroke(1.25, ink),
            );
            painter.line_segment(
                [center - Vec2::new(0.0, 4.5), center + Vec2::new(0.0, 4.5)],
                super::theme::stroke(1.25, ink),
            );
        }

        if response.clicked() {
            ui.memory_mut(|memory| memory.toggle_popup(popup));
        }
        let mut chosen = None;
        egui::popup_below_widget(ui, popup, &response, |ui| {
            ui.set_min_width(170.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            // Front-panel knobs first, then what only direct control reaches.
            for (standard, heading) in [(true, "Standard"), (false, "Hidden (Guessed)")] {
                let mut specs = params::KNOBS
                    .iter()
                    .filter(|spec| spec.standard == standard)
                    .filter(|spec| self.knobs.iter().all(|knob| knob.offset != spec.offset))
                    .peekable();
                if specs.peek().is_none() {
                    continue;
                }
                ui.label(
                    egui::RichText::new(heading)
                        .font(semibold(11.0))
                        .color(Color32::from_rgb(120, 114, 105)),
                );
                for spec in specs {
                    if ui
                        .selectable_label(false, spec.name)
                        .on_hover_text(spec.hint)
                        .clicked()
                    {
                        chosen = Some(spec);
                    }
                }
            }
        });
        if let Some(spec) = chosen {
            add_knob(&mut self.knobs, spec.offset);
        }
    }
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
