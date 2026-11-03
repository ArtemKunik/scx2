//! What each effect looks like: a real pedal, treadle, tape unit, reverb tank
//! or rack processor, chosen per model.
//!
//! The DSP only knows model ids, so the visual identity lives here: enclosure
//! colour, hardware layout and the legends printed on the box.

use eframe::egui::{Color32, Pos2, Rect, Vec2};

use crate::protocol;

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

/// One effect parameter bound to a knob on the pedal.
#[derive(Clone, Copy)]
pub struct Param {
    pub label: &'static str,
    pub offset: usize,
}

impl Pedal {
    /// The parameter bytes the knobs on this pedal edit, in drawing order.
    pub fn params(&self) -> Vec<Param> {
        (0..self.knobs)
            .map(|index| Param {
                label: self.labels.get(index).copied().unwrap_or("PARAM"),
                offset: protocol::OFF_PARAMS + index,
            })
            .collect()
    }

    /// Knob positions for this pedal inside `unit`, in drawing order.
    ///
    /// The paint code and the hit testing both use this, so a knob cannot drift
    /// away from the circle a person is actually grabbing.
    pub fn controls(&self, unit: Rect) -> Vec<Control> {
        match self.form {
            Form::Box => row(unit, unit.top() + unit.height() * 0.33, 11.0, self.knobs),
            Form::Wah => Vec::new(),
            Form::Tape => row(unit, unit.bottom() - 34.0, 8.0, self.knobs),
            Form::Tank => {
                let plate = Rect::from_min_max(
                    unit.left_bottom() + Vec2::new(8.0, -52.0),
                    unit.right_bottom() + Vec2::new(-8.0, -8.0),
                );
                [0.56, 0.72, 0.88]
                    .into_iter()
                    .take(self.knobs)
                    .map(|t| Control {
                        center: Pos2::new(plate.left() + plate.width() * t, plate.center().y + 2.0),
                        radius: 8.5,
                    })
                    .collect()
            }
            Form::Rack => {
                let face = Rect::from_min_max(
                    unit.left_top() + Vec2::new(10.0, 5.0),
                    unit.right_bottom() + Vec2::new(-10.0, -6.0),
                );
                [0.64, 0.82]
                    .into_iter()
                    .take(self.knobs)
                    .map(|t| Control {
                        center: Pos2::new(face.left() + face.width() * t, face.center().y + 10.0),
                        radius: 8.0,
                    })
                    .collect()
            }
        }
    }
}

fn row(unit: Rect, y: f32, radius: f32, count: usize) -> Vec<Control> {
    let span = unit.width() * 0.66;
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
                let params = look.params();
                assert_eq!(params.len(), look.knobs);
                for (index, param) in params.iter().enumerate() {
                    assert_eq!(param.offset, protocol::OFF_PARAMS + index);
                    assert!(
                        unit.contains(controls[index].center),
                        "{name} draws knob {index} outside its unit"
                    );
                }
            }
        }
    }
}
