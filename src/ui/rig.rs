//! The head-and-cab picture in the amp/speaker picker.
//!
//! Like the rest of the hardware, it is drawn instead of shipped as artwork.
//! The stack is a small three-quarter view: the head takes the era finish of
//! the selected voice, the cabinet takes the finish its name implies, and the
//! speaker cones change with the selected cabinet.

use eframe::egui::{Align2, Color32, Painter, Pos2, Rect, Shape, Vec2};

use super::ampstyle::{self, Era, Look};
use super::art::Art;
use super::paint::{full_uv, spaced, tile, vertical_gradient};
use super::theme::*;
use crate::models;

/// Height the picker should reserve for the picture and its captions.
pub(super) const RIG_H: f32 = 214.0;

/// Draw the selected voice's head sitting on the selected cabinet.
pub(super) fn draw(painter: &Painter, rect: Rect, art: &Art, voice: u8, cabinet: u8) {
    let head_look = ampstyle::for_model(models::voice_model_id(voice));
    let cab_look = cab_era(cabinet).look();

    painter.rect_filled(rect, 9.0, Color32::from_rgb(23, 22, 20));
    vertical_gradient(
        painter,
        rect.shrink(2.0),
        Color32::from_rgb(34, 31, 27),
        Color32::from_rgb(14, 13, 12),
    );
    // Warm pool of light on the back wall.
    painter.image(
        art.glow.id(),
        Rect::from_center_size(
            Pos2::new(rect.center().x, rect.center().y - 8.0),
            Vec2::new(rect.width() * 0.92, rect.height() * 0.95),
        ),
        full_uv(),
        Color32::from_rgba_premultiplied(38, 31, 22, 0),
    );

    let floor = rect.bottom() - 46.0;
    let depth = Vec2::new(17.0, -12.0);
    let cab_w: f32 = match arrangement(cabinet) {
        None => 168.0,
        Some((1, _)) => 156.0,
        Some((2, _)) => 188.0,
        _ => 214.0,
    };
    let cab_h = 96.0;
    let cx = rect.center().x - depth.x / 2.0;
    let cab = Rect::from_min_size(
        Pos2::new(cx - cab_w / 2.0, floor - cab_h),
        Vec2::new(cab_w, cab_h),
    );

    // The cabinet's shadow on the stage.
    painter.image(
        art.glow.id(),
        Rect::from_center_size(
            Pos2::new(rect.center().x, floor + 3.0),
            Vec2::new(cab_w * 1.45, 34.0),
        ),
        full_uv(),
        Color32::from_black_alpha(230),
    );

    paint_cabinet(painter, art, &cab_look, cab, depth, cabinet);

    // The head sits on the back half of the cabinet's top face.
    let head_h = 44.0;
    let head_w = (cab_w * 0.78).clamp(140.0, 172.0);
    let base = cab.top() + depth.y * 0.5;
    let head = Rect::from_min_size(
        Pos2::new(cab.center().x + depth.x * 0.5 - head_w / 2.0, base - head_h),
        Vec2::new(head_w, head_h),
    );

    painter.image(
        art.glow.id(),
        Rect::from_center_size(
            Pos2::new(head.center().x, base + 2.0),
            Vec2::new(head_w * 1.2, 18.0),
        ),
        full_uv(),
        Color32::from_black_alpha(180),
    );

    paint_head(painter, art, &head_look, head, depth);

    // A stage edge separates the picture from the captions.
    painter.line_segment(
        [
            Pos2::new(rect.left() + 18.0, floor + 13.0),
            Pos2::new(rect.right() - 18.0, floor + 13.0),
        ],
        stroke(1.0, HAIRLINE),
    );

    let voice_name = models::model_name(crate::protocol::Kind::Amp, models::voice_model_id(voice))
        .unwrap_or("Unknown");
    let speaker_name = models::cabinet_name(cabinet).unwrap_or("Custom cabinet");
    caption(
        painter,
        Pos2::new(rect.left() + 18.0, floor + 20.0),
        Align2::LEFT_TOP,
        "AMP",
        voice_name,
    );
    caption(
        painter,
        Pos2::new(rect.right() - 18.0, floor + 20.0),
        Align2::RIGHT_TOP,
        "SPEAKER",
        speaker_name,
    );
}

fn caption(painter: &Painter, pos: Pos2, align: Align2, label: &str, value: &str) {
    spaced(painter, pos, align, label, semibold(8.5), TEXT_3, 1.4);
    painter.text(
        pos + Vec2::new(0.0, 12.0),
        align,
        value,
        semibold(12.5),
        TEXT,
    );
}

fn paint_head(painter: &Painter, art: &Art, look: &Look, head: Rect, depth: Vec2) {
    let surface = art.surface(look.era);
    let top = vec![
        head.left_top(),
        head.right_top(),
        head.right_top() + depth,
        head.left_top() + depth,
    ];
    let side = vec![
        head.right_top(),
        head.right_top() + depth,
        head.right_bottom() + depth,
        head.right_bottom(),
    ];
    let handle_center = top_center(&top);
    painter.add(Shape::convex_polygon(
        side,
        tolex(look, 0.62),
        stroke(1.0, Color32::from_black_alpha(110)),
    ));
    painter.add(Shape::convex_polygon(
        top,
        tolex(look, 1.38),
        stroke(1.0, look.piping.gamma_multiply(0.35)),
    ));

    // A leather handle sits in the middle of the top face.
    let handle = Rect::from_center_size(handle_center, Vec2::new(28.0, 4.5));
    painter.rect_filled(handle, 2.2, Color32::from_rgb(26, 24, 21));
    painter.rect_stroke(handle, 2.2, stroke(0.8, look.piping.gamma_multiply(0.45)));

    tile(painter, head, 4.0, &surface.tolex, art.ppp, Color32::WHITE);
    painter.rect_stroke(head, 4.0, stroke(1.0, look.piping.gamma_multiply(0.55)));
    painter.line_segment(
        [
            head.left_top() + Vec2::new(6.0, 1.2),
            head.right_top() + Vec2::new(-6.0, 1.2),
        ],
        stroke(0.9, look.piping.gamma_multiply(0.45)),
    );

    spaced(
        painter,
        head.center_top() + Vec2::new(0.0, 5.0),
        Align2::CENTER_TOP,
        "SUPER CHAMP X2",
        semibold(7.0),
        look.piping.gamma_multiply(0.9),
        1.1,
    );

    // Control panel: one jewel, then the row of knobs.
    let panel = Rect::from_min_size(
        Pos2::new(head.left() + 4.0, head.bottom() - 20.0),
        Vec2::new(head.width() - 8.0, 16.0),
    );
    painter.rect_filled(panel.expand(1.0), 2.0, look.piping.gamma_multiply(0.6));
    tile(painter, panel, 2.0, &surface.panel, art.ppp, Color32::WHITE);
    painter.rect_stroke(panel, 2.0, stroke(0.9, look.piping.gamma_multiply(0.75)));

    let jewel = Pos2::new(panel.left() + 8.0, panel.center().y);
    painter.circle_filled(jewel, 3.0, Color32::from_black_alpha(180));
    painter.circle_filled(jewel, 2.2, look.accent);
    painter.circle_filled(
        jewel - Vec2::new(0.6, 0.6),
        0.9,
        Color32::from_white_alpha(160),
    );

    let first = panel.right() - 54.0;
    for index in 0..5 {
        let center = Pos2::new(first + index as f32 * 10.5, panel.center().y);
        painter.circle_filled(center, 3.4, Color32::from_rgb(20, 18, 16));
        painter.circle_stroke(center, 3.4, stroke(0.7, look.piping.gamma_multiply(0.8)));
        painter.line_segment(
            [center, center + Vec2::new(0.0, -2.4)],
            stroke(0.8, look.ink.gamma_multiply(0.85)),
        );
    }
}

fn paint_cabinet(painter: &Painter, art: &Art, look: &Look, cab: Rect, depth: Vec2, cabinet: u8) {
    let surface = art.surface(look.era);
    let top = vec![
        cab.left_top(),
        cab.right_top(),
        cab.right_top() + depth,
        cab.left_top() + depth,
    ];
    let side = vec![
        cab.right_top(),
        cab.right_top() + depth,
        cab.right_bottom() + depth,
        cab.right_bottom(),
    ];
    painter.add(Shape::convex_polygon(
        side,
        tolex(look, 0.58),
        stroke(1.0, Color32::from_black_alpha(110)),
    ));
    painter.add(Shape::convex_polygon(
        top,
        tolex(look, 1.34),
        stroke(1.0, look.piping.gamma_multiply(0.30)),
    ));

    tile(painter, cab, 4.0, &surface.tolex, art.ppp, Color32::WHITE);
    painter.rect_stroke(cab, 4.0, stroke(1.0, look.piping.gamma_multiply(0.45)));

    let grille = cab.shrink2(Vec2::new(8.0, 8.0));
    painter.rect_filled(grille.expand(2.0), 3.0, look.piping.gamma_multiply(0.5));
    tile(
        painter,
        grille,
        3.0,
        &surface.grille,
        art.ppp,
        Color32::WHITE,
    );
    painter.rect_stroke(grille, 3.0, stroke(0.9, look.piping.gamma_multiply(0.8)));

    match arrangement(cabinet) {
        Some((columns, rows)) => {
            let inner = grille.shrink2(Vec2::new(9.0, 7.0));
            let gap = 6.0;
            let side = ((inner.width() - gap * (columns as f32 - 1.0)) / columns as f32)
                .min((inner.height() - gap * (rows as f32 - 1.0)) / rows as f32);
            let total = Vec2::new(
                columns as f32 * side + (columns as f32 - 1.0) * gap,
                rows as f32 * side + (rows as f32 - 1.0) * gap,
            );
            let origin = inner.center() - total / 2.0;
            for row in 0..rows {
                for column in 0..columns {
                    let cell = Rect::from_min_size(
                        origin + Vec2::new(column as f32 * (side + gap), row as f32 * (side + gap)),
                        Vec2::splat(side),
                    );
                    speaker(painter, cell.shrink(side * 0.04));
                }
            }
            // A breath of cloth in front of the cones.
            tile(
                painter,
                grille,
                3.0,
                &surface.grille,
                art.ppp,
                Color32::from_white_alpha(22),
            );
        }
        None => direct(painter, grille),
    }

    // Metal corner protectors.
    for center in [
        cab.left_top() + Vec2::splat(4.0),
        cab.right_top() + Vec2::new(-4.0, 4.0),
        cab.left_bottom() + Vec2::new(4.0, -4.0),
        cab.right_bottom() - Vec2::splat(4.0),
    ] {
        let cap = Rect::from_center_size(center, Vec2::splat(7.0));
        painter.rect_filled(cap, 1.5, Color32::from_rgb(48, 46, 42));
        painter.rect_stroke(cap, 1.5, stroke(0.7, look.piping.gamma_multiply(0.55)));
    }

    // The rail along the bottom of the cabinet.
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(cab.left() + 2.0, cab.bottom() - 4.5),
            Vec2::new(cab.width() - 4.0, 2.5),
        ),
        1.2,
        Color32::from_black_alpha(90),
    );
}

fn speaker(painter: &Painter, cell: Rect) {
    let center = cell.center();
    let radius = cell.width().min(cell.height()) / 2.0;

    // Cutout, basket rim and a cone that brightens toward the dust cap.
    painter.circle_filled(
        center + Vec2::new(0.0, 1.2),
        radius,
        Color32::from_black_alpha(140),
    );
    painter.circle_filled(center, radius, Color32::from_rgb(15, 14, 13));
    painter.circle_stroke(
        center,
        radius * 0.97,
        stroke(1.2, Color32::from_rgb(82, 74, 64)),
    );
    for step in 0..6 {
        let t = step as f32 / 5.0;
        let shade = 28.0 + t * 26.0;
        painter.circle_filled(
            center,
            radius * (0.88 - t * 0.52),
            Color32::from_rgb(shade as u8, (shade * 0.93) as u8, (shade * 0.83) as u8),
        );
    }
    for ring in 0..3 {
        painter.circle_stroke(
            center,
            radius * (0.78 - ring as f32 * 0.15),
            stroke(0.7, Color32::from_black_alpha(75)),
        );
    }
    painter.circle_filled(center, radius * 0.23, Color32::from_rgb(58, 52, 44));
    painter.circle_stroke(
        center,
        radius * 0.23,
        stroke(0.9, Color32::from_rgb(96, 87, 74)),
    );
    painter.circle_filled(
        center - Vec2::new(radius * 0.08, radius * 0.08),
        radius * 0.075,
        Color32::from_white_alpha(24),
    );
}

fn direct(painter: &Painter, grille: Rect) {
    let center = Pos2::new(grille.center().x, grille.center().y - 2.0);
    let radius = (grille.height() * 0.27).min(grille.width() * 0.15);
    let ink = Color32::from_rgb(132, 124, 112);
    painter.circle_stroke(center, radius, stroke(1.3, ink));
    let slash = Vec2::new(radius * 0.72, radius * 0.72);
    painter.line_segment([center - slash, center + slash], stroke(1.3, ink));
    spaced(
        painter,
        Pos2::new(center.x, center.y + radius + 6.0),
        Align2::CENTER_TOP,
        "DIRECT",
        semibold(8.0),
        TEXT_3,
        1.6,
    );
}

/// The finish a cabinet's era suggests: tweed, blackface, British or modern.
fn cab_era(cabinet: u8) -> Era {
    match cabinet {
        1 | 2 | 5 => Era::Tweed,
        3 | 4 | 9 => Era::Blackface,
        6..=8 => Era::British,
        10..=12 => Era::Modern,
        _ => Era::Blackface,
    }
}

fn tolex(look: &Look, shade: f32) -> Color32 {
    let channel = |value: f32| ((value * shade + 0.015).clamp(0.0, 1.0) * 255.0) as u8;
    Color32::from_rgb(
        channel(look.tolex[0]),
        channel(look.tolex[1]),
        channel(look.tolex[2]),
    )
}

fn top_center(quad: &[Pos2]) -> Pos2 {
    let sum = quad
        .iter()
        .fold(Vec2::ZERO, |sum, point| sum + point.to_vec2());
    (sum / quad.len() as f32).to_pos2()
}

/// Speaker grid for FUSE's cabinet ids: `None` means no cabinet (direct).
fn arrangement(cabinet: u8) -> Option<(usize, usize)> {
    if cabinet == 0 {
        return None;
    }
    let name = models::cabinet_name(cabinet)?;
    let count = name.split('x').next()?.parse::<usize>().ok()?;
    Some(match count {
        1 => (1, 1),
        2 => (2, 1),
        _ => (2, 2),
    })
}

#[cfg(test)]
mod tests {
    use super::{arrangement, cab_era};
    use crate::ui::ampstyle::Era;

    #[test]
    fn cabinets_map_to_their_speaker_grids() {
        assert_eq!(arrangement(0), None, "Off is direct");
        assert_eq!(arrangement(1), Some((1, 1)), "1x12 '57 Deluxe");
        assert_eq!(arrangement(7), Some((2, 1)), "2x12 AC30");
        assert_eq!(arrangement(6), Some((2, 2)), "4x12 JCM800");
        assert_eq!(arrangement(2), Some((2, 2)), "4x10 '59 Bassman");
    }

    #[test]
    fn every_named_cabinet_has_a_speaker_grid() {
        for (id, name) in crate::models::CABINET_MODELS {
            if *id == 0 {
                continue;
            }
            assert!(
                arrangement(*id).is_some(),
                "cabinet {id} ({name}) has no speaker arrangement"
            );
        }
    }

    #[test]
    fn cabinets_get_a_finish_that_matches_their_era() {
        assert_eq!(cab_era(1), Era::Tweed);
        assert_eq!(cab_era(6), Era::British);
        assert_eq!(cab_era(10), Era::Modern);
    }
}
