//! Painting helpers and the small hardware: screws, cloth, letter-spaced ink.

use std::sync::Arc;

use eframe::egui::{
    epaint::{Galley, RectShape},
    text::{LayoutJob, TextFormat, TextWrapping},
    Align2, Color32, FontId, Mesh, Painter, Pos2, Rect, Shape, TextureHandle, Vec2,
};

use super::ampstyle::{Era, Look};
use super::art::Art;
use super::theme::*;

pub(super) fn screw(painter: &Painter, center: Pos2) {
    painter.circle_filled(
        center + Vec2::new(0.0, 0.75),
        3.75,
        Color32::from_black_alpha(160),
    );
    painter.circle_filled(center, 3.25, Color32::from_rgb(112, 110, 106));
    painter.circle_filled(
        center - Vec2::new(0.5, 0.75),
        2.25,
        Color32::from_rgb(146, 144, 139),
    );
    let slot = Vec2::new(2.2, -1.2);
    painter.line_segment(
        [center - slot, center + slot],
        stroke(1.0, Color32::from_rgb(40, 39, 38)),
    );
}

pub(super) fn paint_grille(
    painter: &Painter,
    art: &Art,
    look: &Look,
    grille: Rect,
    model_name: &str,
) {
    let surface = art.surface(look.era);
    painter.rect_filled(grille.expand(3.0), 6.0, Color32::from_rgb(7, 7, 7));
    painter.rect_filled(grille.expand(1.5), 5.0, look.piping.gamma_multiply(0.55));
    tile(
        painter,
        grille,
        4.0,
        &surface.grille,
        art.ppp,
        Color32::WHITE,
    );
    painter.rect_stroke(grille.shrink(1.0), 3.0, stroke(1.0, look.piping));

    // The clipped painter keeps these soft shapes on the cloth.
    let cloth = painter.with_clip_rect(painter.clip_rect().intersect(grille));

    // A 10" speaker sitting behind the cloth, and the panel's cast shadow.
    let speaker = Rect::from_center_size(
        grille.center(),
        Vec2::new(grille.width() * 1.15, grille.height().max(120.0) * 2.4),
    );
    cloth.image(
        art.glow.id(),
        speaker,
        full_uv(),
        Color32::from_black_alpha(70),
    );
    let lip = Rect::from_min_size(grille.min, Vec2::new(grille.width(), 26.0));
    vertical_gradient(
        &cloth,
        lip,
        Color32::from_black_alpha(150),
        Color32::TRANSPARENT,
    );

    // The classic script wordmark, in the era's metal.
    spaced_italic(
        &cloth,
        grille.left_top() + Vec2::new(20.0, 16.0),
        Align2::LEFT_TOP,
        "Fender",
        semibold(21.0),
        look.piping,
        0.4,
    );

    // A small model plate, riveted to the cloth.
    let plate = Rect::from_min_max(
        grille.right_bottom() + Vec2::new(-190.0, -48.0),
        grille.right_bottom() + Vec2::new(-16.0, -14.0),
    );
    cloth.rect_filled(
        plate.translate(Vec2::new(0.0, 1.5)).expand(1.0),
        3.0,
        Color32::from_black_alpha(170),
    );
    cloth.rect_filled(plate, 3.0, Color32::from_rgb(150, 148, 142));
    // A chrome badge on every era, so the dark panel of a blackface or a
    // metal amp does not swallow its own name.
    tile(
        &cloth,
        plate.shrink(1.0),
        2.5,
        &art.surface(Era::Silverface).panel,
        art.ppp,
        Color32::WHITE,
    );
    spaced(
        &cloth,
        plate.center_top() + Vec2::new(0.0, 5.0),
        Align2::CENTER_TOP,
        &model_name.to_uppercase(),
        semibold(10.5),
        Color32::from_rgb(28, 26, 24),
        1.5,
    );
    spaced(
        &cloth,
        plate.center_bottom() + Vec2::new(0.0, -5.0),
        Align2::CENTER_BOTTOM,
        &look.tag().to_uppercase(),
        regular(8.0),
        Color32::from_rgb(72, 68, 62),
        0.8,
    );
}

pub(super) fn full_uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}

/// Align to physical pixels so 1:1 textures stay crisp.
pub(super) fn snapped(rect: Rect, ppp: f32) -> Rect {
    let min = Pos2::new(
        (rect.min.x * ppp).round() / ppp,
        (rect.min.y * ppp).round() / ppp,
    );
    Rect::from_min_size(min, rect.size())
}

/// Fill a rounded rectangle with a repeating texture at 1:1 physical pixels.
pub(super) fn tile(
    painter: &Painter,
    rect: Rect,
    rounding: f32,
    texture: &TextureHandle,
    ppp: f32,
    tint: Color32,
) {
    let [tex_w, tex_h] = texture.size();
    let mut shape = RectShape::filled(rect, rounding, tint);
    shape.fill_texture_id = texture.id();
    shape.uv = Rect::from_min_size(
        Pos2::ZERO,
        Vec2::new(
            rect.width() * ppp / tex_w as f32,
            rect.height() * ppp / tex_h as f32,
        ),
    );
    painter.add(shape);
}

pub(super) fn vertical_gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// Letter-spaced text, for silk-screened caps.
pub(super) fn spaced(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
) {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    let galley = painter.layout_job(job);
    let rect = align.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
}

/// Letter-spaced italic, for the amplifier's script wordmark.
pub(super) fn spaced_italic(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
) {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: spacing,
            italics: true,
            ..Default::default()
        },
    );
    let galley = painter.layout_job(job);
    let rect = align.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
}

/// One line of text ending in an ellipsis when it does not fit.
pub(super) fn elided(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping {
        max_width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}
