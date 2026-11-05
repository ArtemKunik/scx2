//! What each effect looks like: a real pedal, treadle, tape unit, reverb tank
//! or rack processor, chosen per model.
//!
//! The DSP only knows model ids, so the visual identity lives here: enclosure
//! colour, hardware layout and the legends printed on the box.

use eframe::egui::{Color32, Pos2, Rect, Vec2};

pub(super) use super::fxmodels::look;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A compact stompbox.
    Box,
    /// A treadle, for wah.
    Wah,
    /// A tape echo with visible reels.
    Tape,
    /// A Fender-style outboard spring reverb.
    Tank,
    /// A 2U rack processor.
    Rack,
}

pub struct Pedal {
    pub form: Form,
    pub body: Color32,
    pub edge: Color32,
    pub face: Color32,
    pub ink: Color32,
    pub knobs: usize,
    pub knob: Color32,
    pub knob_cap: Color32,
    pub led: Color32,
    pub accent: Color32,
    pub brand: &'static str,
    pub name: &'static str,
    pub sub: &'static str,
    pub labels: &'static [&'static str],
}

/// Where one knob is drawn, in screen coordinates.
#[derive(Clone, Copy)]
pub struct Control {
    pub center: Pos2,
    pub radius: f32,
}

impl Pedal {
    /// Knob positions for this pedal inside `unit`, in drawing order.
    ///
    /// The paint code and the hit testing both use this, so a knob cannot drift
    /// away from the circle a person is actually grabbing.
    pub fn controls(&self, unit: Rect) -> Vec<Control> {
        // FUSE gives every effect six parameter slots. Five- and six-knob
        // models get two rows on the board so their knobs stay visible instead
        // of collapsing into a line of dots.
        let radius = if self.knobs > 4 { 8.5 } else { 11.0 };
        match self.form {
            Form::Box => {
                if self.knobs > 4 {
                    let split = self.knobs.div_ceil(2);
                    let mut controls =
                        row_of(unit, unit.top() + unit.height() * 0.27, radius, split);
                    controls.extend(row_of(
                        unit,
                        unit.top() + unit.height() * 0.42,
                        radius,
                        self.knobs - split,
                    ));
                    controls
                } else {
                    row(unit, unit.top() + unit.height() * 0.33, radius, self.knobs)
                }
            }
            Form::Wah => Vec::new(),
            Form::Tape => {
                let radius = if self.knobs > 4 { 6.0 } else { 8.0 };
                row(unit, unit.bottom() - 34.0, radius, self.knobs)
            }
            Form::Tank => {
                let plate = Rect::from_min_max(
                    unit.left_bottom() + Vec2::new(8.0, -52.0),
                    unit.right_bottom() + Vec2::new(-8.0, -8.0),
                );
                row(plate, plate.center().y + 2.0, 6.5, self.knobs)
            }
            Form::Rack => {
                let face = Rect::from_min_max(
                    unit.left_top() + Vec2::new(10.0, 5.0),
                    unit.right_bottom() + Vec2::new(-10.0, -6.0),
                );
                row(face, face.center().y + 10.0, 7.0, self.knobs)
            }
        }
    }
}

fn row(unit: Rect, y: f32, radius: f32, count: usize) -> Vec<Control> {
    row_of(unit, y, radius, count)
}

fn row_of(unit: Rect, y: f32, radius: f32, count: usize) -> Vec<Control> {
    let span = unit.width() * 0.72;
    (0..count)
        .map(|index| {
            let t = if count == 1 {
                0.5
            } else {
                index as f32 / (count - 1) as f32
            };
            Control {
                center: Pos2::new(unit.center().x - span / 2.0 + span * t, y),
                radius,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models;
    use crate::protocol::Kind;

    #[test]
    fn every_effect_model_draws_the_controls_it_binds() {
        let unit = Rect::from_min_size(Pos2::ZERO, Vec2::new(260.0, 180.0));
        for kind in [Kind::Stomp, Kind::Mod, Kind::Delay, Kind::Reverb] {
            for (model, name) in models::table(kind)
                .unwrap()
                .iter()
                .filter(|(model, _)| *model != 0)
            {
                let look = look(kind, *model)
                    .unwrap_or_else(|| panic!("{kind:?} model {name} has no face"));
                let controls = look.controls(unit);
                assert_eq!(
                    controls.len(),
                    look.knobs,
                    "{name} draws a different number of knobs than it binds"
                );
                assert!(
                    look.labels.len() >= look.knobs,
                    "{name} has a knob without a label"
                );
                for (index, control) in controls.iter().enumerate() {
                    assert!(
                        unit.contains(control.center),
                        "{name} draws knob {index} outside its unit"
                    );
                }
            }
        }
    }

    #[test]
    fn five_knob_pedals_use_two_readable_rows() {
        let unit = Rect::from_min_size(Pos2::ZERO, Vec2::new(164.0, 180.0));
        let look = look(Kind::Stomp, 0x003c).expect("Overdrive has a face");
        assert_eq!(look.knobs, 5);
        let controls = look.controls(unit);
        assert_eq!(controls.len(), 5);
        assert!(
            controls[0].center.y < controls[3].center.y,
            "the fifth knob should drop to a second row"
        );
        assert!(
            (controls[0].center.y - controls[2].center.y).abs() < 1.0,
            "the first three knobs should share the top row"
        );
        assert!(
            (controls[3].center.y - controls[4].center.y).abs() < 1.0,
            "the last two knobs should share the second row"
        );
        for control in &controls {
            assert!(unit.contains(control.center));
        }
    }
}
