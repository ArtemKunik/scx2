//! Materials for the amplifier, rendered once per pixel density and era.
//!
//! Everything is computed instead of shipped as image assets, so the binary
//! needs no artwork and every texture lands 1:1 on physical pixels.

use eframe::egui::{Context, TextureHandle, TextureOptions};

use super::ampstyle::{Era, JewelTint, KnobFinish, ERAS};
use super::{parts, texture};

/// Knob diameter in points.
pub const KNOB_D: f32 = 68.0;
/// Jewel diameter in points.
pub const JEWEL_D: f32 = 26.0;

pub struct Surface {
    pub tolex: TextureHandle,
    pub panel: TextureHandle,
    pub grille: TextureHandle,
}

pub struct Art {
    pub ppp: f32,
    surfaces: Vec<Surface>,
    knobs: Vec<TextureHandle>,
    jewels: Vec<TextureHandle>,
    /// White radial falloff: tint black for a shadow, or use a zero-alpha tint
    /// for additive light.
    pub glow: TextureHandle,
}

impl Art {
    pub fn new(ctx: &Context, ppp: f32) -> Self {
        let tile = TextureOptions::LINEAR_REPEAT;
        let once = TextureOptions::LINEAR;
        let surfaces = ERAS
            .iter()
            .map(|era| {
                let look = era.look();
                let name = era.label();
                Surface {
                    tolex: ctx.load_texture(
                        format!("tolex-{name}"),
                        texture::tolex(ppp, look.tolex, look.weave),
                        tile,
                    ),
                    panel: ctx.load_texture(
                        format!("panel-{name}"),
                        texture::panel(look.panel),
                        tile,
                    ),
                    grille: ctx.load_texture(
                        format!("grille-{name}"),
                        texture::grille(ppp, look.cloth),
                        tile,
                    ),
                }
            })
            .collect();
        let knobs = [
            KnobFinish::BlackChrome,
            KnobFinish::Cream,
            KnobFinish::Silver,
            KnobFinish::Gold,
        ]
        .into_iter()
        .map(|finish| {
            ctx.load_texture(
                format!("knob-{}", finish.index()),
                parts::knob((KNOB_D * ppp).ceil() as usize, finish),
                once,
            )
        })
        .collect();
        let jewels = [
            (JewelTint::Red, [0.96, 0.11, 0.07]),
            (JewelTint::Blue, [0.16, 0.42, 1.0]),
            (JewelTint::Amber, [1.0, 0.54, 0.12]),
        ]
        .into_iter()
        .map(|(tint, rgb)| {
            ctx.load_texture(
                format!("jewel-{}", tint.index()),
                parts::jewel((JEWEL_D * ppp).ceil() as usize, rgb),
                once,
            )
        })
        .collect();
        Art {
            ppp,
            surfaces,
            knobs,
            jewels,
            glow: ctx.load_texture("glow", parts::glow(128), once),
        }
    }

    pub fn surface(&self, era: Era) -> &Surface {
        &self.surfaces[era.index()]
    }

    pub fn knob(&self, finish: KnobFinish) -> &TextureHandle {
        &self.knobs[finish.index()]
    }

    pub fn jewel(&self, tint: JewelTint) -> &TextureHandle {
        &self.jewels[tint.index()]
    }
}
