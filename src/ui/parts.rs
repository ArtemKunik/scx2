//! Small hardware: knobs, pilot jewels and the soft light falloff used for
//! shadows and bloom.

use eframe::egui::{Color32, ColorImage};
use std::f32::consts::{PI, TAU};

use super::ampstyle::KnobFinish;
use super::texture::{hash, to_u8};

const LIGHT: [f32; 3] = [-0.42, -0.68, 0.6];

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normal(x: f32, y: f32, z: f32) -> [f32; 3] {
    let len = (x * x + y * y + z * z).sqrt();
    [x / len, y / len, z / len]
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Render a radial shape with 3x3 supersampling.
fn radial(size: usize, shade: impl Fn(f32, f32) -> [f32; 3]) -> ColorImage {
    const SS: usize = 3;
    let half = size as f32 / 2.0;
    let mut pixels = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0f32; 3];
            let mut covered = 0.0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let dx = (x as f32 + (sx as f32 + 0.5) / SS as f32 - half) / half;
                    let dy = (y as f32 + (sy as f32 + 0.5) / SS as f32 - half) / half;
                    let r = (dx * dx + dy * dy).sqrt();
                    if r <= 0.985 {
                        let c = shade(r, dy.atan2(dx));
                        sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
                        covered += 1.0;
                    }
                }
            }
            let count = (SS * SS) as f32;
            pixels.push(Color32::from_rgba_premultiplied(
                to_u8(sum[0] / count),
                to_u8(sum[1] / count),
                to_u8(sum[2] / count),
                to_u8(covered / count),
            ));
        }
    }
    ColorImage {
        size: [size, size],
        pixels,
    }
}

/// A skirted knob in one of the era's finishes.
///
/// The highlights belong to the room's light, not to the knob, so the body
/// never rotates; only the pointer drawn over it does.
pub fn knob(size: usize, finish: KnobFinish) -> ColorImage {
    let (body, top, rim) = match finish {
        KnobFinish::BlackChrome => (
            [0.085, 0.083, 0.080],
            [0.640, 0.630, 0.610],
            [0.740, 0.730, 0.710],
        ),
        KnobFinish::Cream => (
            [0.620, 0.540, 0.390],
            [0.800, 0.740, 0.600],
            [0.900, 0.845, 0.710],
        ),
        KnobFinish::Silver => (
            [0.360, 0.362, 0.370],
            [0.740, 0.748, 0.756],
            [0.840, 0.848, 0.856],
        ),
        KnobFinish::Gold => (
            [0.360, 0.278, 0.098],
            [0.740, 0.615, 0.260],
            [0.850, 0.730, 0.360],
        ),
    };
    let light = normal(LIGHT[0], LIGHT[1], LIGHT[2]);
    let half_vec = normal(light[0], light[1], light[2] + 1.0);
    radial(size, |r, a| {
        const CAP: f32 = 0.6;
        const GRIP: f32 = 0.7;
        if r < CAP {
            let beam = |at: f32, power: f32| ((a - at).cos().abs()).powf(power);
            let v = 0.56
                + 0.34 * beam(-2.2, 18.0)
                + 0.14 * beam(-1.1, 40.0)
                + 0.10 * (2.0 * (a + 0.9)).cos()
                + 0.012 * (r * 260.0).sin();
            let gain = v / 0.56;
            let rim_t = smooth(CAP - 0.05, CAP - 0.01, r);
            let lit = dot(normal(a.cos(), a.sin(), 0.8), light);
            let edge = 0.70 + lit * 0.55;
            let mut c = [top[0] * gain, top[1] * gain, top[2] * gain];
            for channel in 0..3 {
                c[channel] = c[channel] * (1.0 - rim_t) + rim[channel] * edge * rim_t;
            }
            c
        } else if r < GRIP {
            let flute = (a * 56.0).cos();
            let lit = dot(normal(a.cos() + flute * 0.5, a.sin(), 0.4), light).max(0.0);
            let shadow = smooth(CAP, CAP + 0.015, r);
            let shade = (0.05 + lit * 0.48) * shadow;
            [body[0] * shade, body[1] * shade, body[2] * shade]
        } else {
            let lip = smooth(0.9, 0.985, r);
            let slope = 0.42 + lip * 1.1;
            let n = normal(a.cos() * slope, a.sin() * slope, 1.0);
            let diffuse = dot(n, light).max(0.0);
            let spec = dot(n, half_vec).max(0.0).powf(34.0);
            let shade = 0.10 + diffuse * 0.55;
            let white = spec * 0.30;
            [
                body[0] * shade + white,
                body[1] * shade + white,
                body[2] * shade + white,
            ]
        }
    })
}

/// The faceted pilot jewel in a chrome bezel, tinted to match the era.
pub fn jewel(size: usize, tint: [f32; 3]) -> ColorImage {
    let light = normal(LIGHT[0], LIGHT[1], LIGHT[2]);
    radial(size, |r, a| {
        if r > 0.74 {
            let n = normal(a.cos(), a.sin(), 0.7);
            let v = 0.2 + dot(n, light).max(0.0) * 0.75;
            return [v, v, v * 0.97];
        }
        let sector = ((a + PI) / TAU * 12.0) as u32;
        let ring = (r / 0.74 * 3.0) as u32;
        let facet = hash((sector + ring) % 2, ring, 9);
        let mid = (sector as f32 + 0.5) / 12.0 * TAU - PI;
        let wedge = dot(normal(mid.cos(), mid.sin(), 1.3), light).max(0.0);
        let b = 0.34 + 0.62 * wedge + 0.18 * facet + (1.0 - r / 0.74) * 0.34;
        let gain = b * 1.28;
        [
            (tint[0] * gain).min(1.0),
            (tint[1] * gain).min(1.0),
            (tint[2] * gain).min(1.0),
        ]
    })
}

/// White radial falloff: tint black for a shadow, or a zero-alpha tint for
/// additive light.
pub fn glow(size: usize) -> ColorImage {
    let half = size as f32 / 2.0;
    let pixels = (0..size * size)
        .map(|i| {
            let dx = ((i % size) as f32 + 0.5 - half) / half;
            let dy = ((i / size) as f32 + 0.5 - half) / half;
            let falloff = (1.0 - (dx * dx + dy * dy).sqrt()).max(0.0);
            let a = to_u8(falloff * falloff * (3.0 - 2.0 * falloff));
            Color32::from_rgba_premultiplied(a, a, a, a)
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}
