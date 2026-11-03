//! Procedural materials: tolex, faceplates and grille cloth.
//!
//! Rendered once per pixel density and per era, so every thread and grain
//! lands on a physical pixel instead of being stretched from one bitmap.

use eframe::egui::{Color32, ColorImage};
use std::f32::consts::PI;

use super::ampstyle::{Cloth, Panel, Weave};

pub fn hash(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add(y.wrapping_mul(0x85EB_CA77))
        .wrapping_add(seed.wrapping_mul(0xC2B2_AE3D));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFF_FFFF) as f32 / 0xFF_FFFF as f32
}

pub fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn opaque(r: f32, g: f32, b: f32) -> Color32 {
    Color32::from_rgb(to_u8(r), to_u8(g), to_u8(b))
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Covering for the cabinet. `base` is the colour of the material itself.
pub fn tolex(ppp: f32, base: [f32; 3], weave: Weave) -> ColorImage {
    let size = 256usize;
    if weave == Weave::Tweed {
        return tweed(size, ppp, base);
    }
    let spacing = if weave == Weave::Levant { 8.5 } else { 4.5 };
    let cells = (size as f32 / (spacing * ppp)).round().max(6.0) as usize;
    let cell = size as f32 / cells as f32;
    let mut height = vec![0f32; size * size];
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 / cell, y as f32 / cell);
            let (cx, cy) = (px.floor() as i32, py.floor() as i32);
            let mut nearest = f32::MAX;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (cx + dx, cy + dy);
                    let wx = nx.rem_euclid(cells as i32) as u32;
                    let wy = ny.rem_euclid(cells as i32) as u32;
                    let fx = nx as f32 + hash(wx, wy, 1) - px;
                    let fy = ny as f32 + hash(wx, wy, 2) - py;
                    nearest = nearest.min(fx * fx + fy * fy);
                }
            }
            height[y * size + x] = 1.0 - smooth(0.0, 0.78, nearest.sqrt());
        }
    }
    let at = |x: usize, y: usize| height[(y % size) * size + (x % size)];
    let pixels = (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            let slope = (at(x + size - 1, y + size - 1) - at(x + 1, y + 1)) * 1.6;
            let grain = hash(x as u32, y as u32, 13) - 0.5;
            let f = 1.0 + slope * 0.55 + at(x, y) * 0.18 + grain * 0.05;
            opaque(
                base[0] * f + grain * 0.006,
                base[1] * f + grain * 0.005,
                base[2] * f + grain * 0.004,
            )
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}

/// Lacquered basket weave, warm at the crown of each thread and dark in the
/// gaps where the varnish has sunk in.
fn tweed(size: usize, ppp: f32, base: [f32; 3]) -> ColorImage {
    let p = (3.0 * ppp).round().max(2.0) as usize;
    let pixels = (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            let (cx, cy) = (x / p, y / p);
            let over = (cx + cy) % 2 == 0;
            let (across, thread) = if over {
                ((y % p) as f32, cy as u32 + 40)
            } else {
                ((x % p) as f32, cx as u32)
            };
            let profile = (PI * (across + 0.5) / p as f32).sin().powf(0.7);
            let tone = 0.78 + hash(thread, over as u32, 7) * 0.34;
            let slub = if hash(cx as u32, cy as u32, 9) > 0.93 {
                -0.10
            } else {
                0.0
            };
            let sheen = 0.035 * (((x + y) as f32 * 0.045).sin());
            let f = 0.58 + 0.58 * profile * tone + slub + sheen;
            let grain = (hash(x as u32, y as u32, 10) - 0.5) * 0.06;
            opaque(
                base[0] * f + grain,
                base[1] * f + grain * 0.8,
                base[2] * f + grain * 0.5,
            )
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}

/// Anodised, brushed or painted metal faceplate with a faint horizontal grain.
pub fn panel(kind: Panel) -> ColorImage {
    let (base, grain) = match kind {
        Panel::Black => ([0.098, 0.096, 0.094], 0.16),
        Panel::Brushed => ([0.66, 0.662, 0.658], 0.10),
        Panel::Gold => ([0.43, 0.32, 0.105], 0.12),
        Panel::Brown => ([0.235, 0.148, 0.088], 0.12),
        Panel::Gunmetal => ([0.132, 0.136, 0.142], 0.14),
    };
    let size = 256usize;
    let pixels = (0..size * size)
        .map(|i| {
            let (x, y) = ((i % size) as u32, (i / size) as u32);
            let streak = hash(0, y, 3) - 0.5;
            let bucket = x / 32;
            let t = (x % 32) as f32 / 32.0;
            let along = hash(bucket, y, 4) * (1.0 - t) + hash((bucket + 1) % 8, y, 4) * t - 0.5;
            let sparkle = (hash(x, y, 11) - 0.5) * 0.035;
            let f = 1.0 + (streak * 0.55 + along * 0.45) * grain + sparkle;
            opaque(base[0] * f, base[1] * f, base[2] * f)
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}

/// Grille cloth. Basket weaves carry most of the era's look; British cabinets
/// get the salt-and-pepper check instead.
pub fn grille(ppp: f32, cloth: Cloth) -> ColorImage {
    let (lo, hi, fleck) = match cloth {
        Cloth::Silver => ([0.105, 0.112, 0.122], [0.690, 0.692, 0.680], 0.16),
        Cloth::Oxblood => ([0.055, 0.015, 0.010], [0.295, 0.092, 0.048], 0.18),
        Cloth::Wheat => ([0.185, 0.152, 0.096], [0.690, 0.600, 0.405], 0.14),
        Cloth::Checker => ([0.028, 0.028, 0.026], [0.560, 0.556, 0.530], 0.10),
        Cloth::Black => ([0.012, 0.012, 0.014], [0.115, 0.118, 0.122], 0.22),
    };
    let p = match cloth {
        Cloth::Checker => (2.5 * ppp).round().max(3.0) as usize,
        Cloth::Black => (1.6 * ppp).round().max(2.0) as usize,
        _ => (2.0 * ppp).round().max(3.0) as usize,
    };
    let size = (256 / (2 * p)) * 2 * p;
    let pixels = (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            let (cx, cy) = (x / p, y / p);
            let over = (cx + cy) % 2 == 0;
            let (across, thread) = if over {
                ((y % p) as f32, cy as u32)
            } else {
                ((x % p) as f32, cx as u32 + 7000)
            };
            let profile = (PI * (across + 0.5) / p as f32).sin().powf(0.8);
            let mut tone = 0.86 + hash(thread, over as u32, 5) * 0.24;
            if cloth == Cloth::Checker {
                tone *= if over { 1.0 } else { 0.72 };
            }
            let t = (profile * tone).min(1.0);
            let dot = if hash(x as u32, y as u32, 6) > 0.992 {
                fleck
            } else {
                0.0
            };
            opaque(
                lo[0] + (hi[0] - lo[0]) * t + dot,
                lo[1] + (hi[1] - lo[1]) * t + dot * 0.9,
                lo[2] + (hi[2] - lo[2]) * t + dot * 0.7,
            )
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}
