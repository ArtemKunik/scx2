//! Procedural amp materials, rendered once per pixel density.
//!
//! Everything is computed instead of shipped as image assets, so the binary
//! needs no artwork and every texture lands 1:1 on physical pixels.

use eframe::egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};
use std::f32::consts::{PI, TAU};

/// Knob diameter in points.
pub const KNOB_D: f32 = 68.0;
/// Jewel diameter in points.
pub const JEWEL_D: f32 = 26.0;

pub struct Art {
    pub ppp: f32,
    pub tolex: TextureHandle,
    pub panel: TextureHandle,
    pub grille: TextureHandle,
    pub knob: TextureHandle,
    pub jewel: TextureHandle,
    /// White radial falloff: tint black for a shadow, or use a zero-alpha tint
    /// for additive light.
    pub glow: TextureHandle,
}

impl Art {
    pub fn new(ctx: &Context, ppp: f32) -> Self {
        let tile = TextureOptions::LINEAR_REPEAT;
        let once = TextureOptions::LINEAR;
        Art {
            ppp,
            tolex: ctx.load_texture("tolex", tolex(ppp), tile),
            panel: ctx.load_texture("panel", panel(), tile),
            grille: ctx.load_texture("grille", grille(ppp), tile),
            knob: ctx.load_texture("knob", knob((KNOB_D * ppp).ceil() as usize), once),
            jewel: ctx.load_texture("jewel", jewel((JEWEL_D * ppp).ceil() as usize), once),
            glow: ctx.load_texture("glow", glow(128), once),
        }
    }
}

fn hash(x: u32, y: u32, seed: u32) -> f32 {
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

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn opaque(r: f32, g: f32, b: f32) -> Color32 {
    Color32::from_rgb(to_u8(r), to_u8(g), to_u8(b))
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Pebbled vinyl: a tileable cell pattern lit from the upper left.
fn tolex(ppp: f32) -> ColorImage {
    let cells = (256.0 / (4.5 * ppp)).round().max(8.0) as usize;
    let size = 256usize;
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
            height[y * size + x] = 1.0 - smooth(0.0, 0.75, nearest.sqrt());
        }
    }
    let at = |x: usize, y: usize| height[(y % size) * size + (x % size)];
    let pixels = (0..size * size)
        .map(|i| {
            let (x, y) = (i % size, i / size);
            let slope = at(x + size - 1, y + size - 1) - at(x + 1, y + 1);
            let v = 0.082 + slope * 0.05 + at(x, y) * 0.014;
            opaque(v, v * 0.975, v * 0.95)
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}

/// Black anodised metal with a faint horizontal grain.
fn panel() -> ColorImage {
    let size = 256usize;
    let pixels = (0..size * size)
        .map(|i| {
            let (x, y) = ((i % size) as u32, (i / size) as u32);
            let streak = hash(0, y, 3) - 0.5;
            let bucket = x / 32;
            let t = (x % 32) as f32 / 32.0;
            let along = hash(bucket, y, 4) * (1.0 - t) + hash((bucket + 1) % 8, y, 4) * t - 0.5;
            let v = 0.098 + streak * 0.012 + along * 0.008;
            opaque(v, v * 0.98, v * 0.96)
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}

/// Silver grille cloth: a basket weave with per-thread variation and a few
/// metallic flecks.
fn grille(ppp: f32) -> ColorImage {
    let p = (2.0 * ppp).round().max(3.0) as usize;
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
            let tone = 0.88 + hash(thread, over as u32, 5) * 0.2;
            let fleck = if hash(x as u32, y as u32, 6) > 0.992 {
                0.16
            } else {
                0.0
            };
            let v = (0.13 + 0.56 * profile * tone + fleck).min(1.0);
            opaque(v, v * 0.985, v * 0.95)
        })
        .collect();
    ColorImage {
        size: [size, size],
        pixels,
    }
}

const LIGHT: [f32; 3] = [-0.42, -0.68, 0.6];

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normal(x: f32, y: f32, z: f32) -> [f32; 3] {
    let len = (x * x + y * y + z * z).sqrt();
    [x / len, y / len, z / len]
}

/// Render a radial shape with 3x3 supersampling. `shade` takes the radius
/// (0..1) and angle and returns r, g, b.
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

/// A black skirted knob with a spun-aluminium top.
///
/// The highlights belong to the room's light, not to the knob, so the body
/// never rotates; only the pointer drawn over it does.
fn knob(size: usize) -> ColorImage {
    let light = normal(LIGHT[0], LIGHT[1], LIGHT[2]);
    let half_vec = normal(light[0], light[1], light[2] + 1.0);
    radial(size, |r, a| {
        const CAP: f32 = 0.6;
        const GRIP: f32 = 0.7;
        if r < CAP {
            // Conical reflections of a spun disc, plus fine lathe rings.
            let beam = |at: f32, power: f32| ((a - at).cos().abs()).powf(power);
            let mut v = 0.56
                + 0.34 * beam(-2.2, 18.0)
                + 0.14 * beam(-1.1, 40.0)
                + 0.10 * (2.0 * (a + 0.9)).cos()
                + 0.012 * (r * 260.0).sin();
            // Chamfered rim catches the light on the upper left.
            let rim = smooth(CAP - 0.05, CAP - 0.01, r);
            let lit = dot(normal(a.cos(), a.sin(), 0.8), light);
            v = v * (1.0 - rim) + (0.42 + lit * 0.55) * rim;
            [v, v * 0.99, v * 0.97]
        } else if r < GRIP {
            let flute = (a * 56.0).cos();
            let lit = dot(normal(a.cos() + flute * 0.5, a.sin(), 0.4), light).max(0.0);
            let shadow = smooth(CAP, CAP + 0.015, r);
            let v = (0.02 + lit * 0.16) * shadow;
            [v, v, v * 0.98]
        } else {
            // The skirt slopes away from the shaft and rolls off at its lip.
            let lip = smooth(0.9, 0.985, r);
            let slope = 0.42 + lip * 1.1;
            let n = normal(a.cos() * slope, a.sin() * slope, 1.0);
            let diffuse = dot(n, light).max(0.0);
            let spec = dot(n, half_vec).max(0.0).powf(34.0);
            let v = 0.016 + diffuse * 0.075 + spec * 0.34;
            [v, v, v * 0.98]
        }
    })
}

/// The faceted pilot jewel in a chrome bezel.
fn jewel(size: usize) -> ColorImage {
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
        [(0.95 * b).min(1.0), 0.13 * b * b, 0.08 * b * b]
    })
}

fn glow(size: usize) -> ColorImage {
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
