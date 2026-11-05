//! The pedalboard the effect hardware sits on.

use eframe::egui::{Color32, Painter, Rect};

use super::ampstyle::Era;
use super::art::Art;
use super::paint::{screw, tile};
use super::theme::{stroke, HAIRLINE};

pub(super) fn paint(painter: &Painter, art: &Art, board: Rect) {
    painter.rect_filled(board, 12.0, Color32::from_rgb(23, 22, 20));
    tile(
        painter,
        board,
        12.0,
        &art.surface(Era::Metal).grille,
        art.ppp,
        Color32::from_gray(96),
    );
    painter.rect_stroke(board, 12.0, stroke(1.0, HAIRLINE));
    for corner in [
        board.left_top() + eframe::egui::Vec2::new(10.0, 10.0),
        board.right_top() + eframe::egui::Vec2::new(-10.0, 10.0),
        board.left_bottom() + eframe::egui::Vec2::new(10.0, -10.0),
        board.right_bottom() + eframe::egui::Vec2::new(-10.0, -10.0),
    ] {
        screw(painter, corner);
    }
}
