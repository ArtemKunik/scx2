//! Palette, type and egui style.
//!
//! One accent only: the amber of a warm tube. It marks what needs attention
//! (selection, unsent edits, the primary action) and nothing else.

use eframe::egui::{
    self, style::ScrollStyle, Color32, FontData, FontDefinitions, FontFamily, FontId, Margin,
    Rounding, Stroke, Vec2,
};

pub const WINDOW: Color32 = Color32::from_rgb(20, 19, 18);
pub const SIDEBAR: Color32 = Color32::from_rgb(27, 26, 24);
pub const RAISED: Color32 = Color32::from_rgb(38, 36, 34);
pub const HAIRLINE: Color32 = Color32::from_rgb(48, 46, 43);
pub const TEXT: Color32 = Color32::from_rgb(241, 237, 229);
pub const TEXT_2: Color32 = Color32::from_rgb(166, 160, 150);
pub const TEXT_3: Color32 = Color32::from_rgb(120, 114, 105);
pub const AMBER: Color32 = Color32::from_rgb(255, 178, 74);
pub const AMBER_INK: Color32 = Color32::from_rgb(36, 22, 4);
pub const ERROR: Color32 = Color32::from_rgb(240, 120, 104);

/// Silk-screen ink on the black control panel.
pub const SILK: Color32 = Color32::from_rgb(226, 222, 212);
pub const SILK_DIM: Color32 = Color32::from_rgb(158, 154, 146);

pub fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke { width, color }
}

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

/// Use the platform's own typeface when it has one. It is read at runtime, so
/// nothing gets redistributed, and egui's built-in fonts remain the fallback.
fn system_font(file: &str) -> Option<FontData> {
    let root = std::env::var("SystemRoot").ok()?;
    let bytes = std::fs::read(std::path::Path::new(&root).join("Fonts").join(file)).ok()?;
    Some(FontData::from_owned(bytes))
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families[&FontFamily::Proportional].clone();

    for (family, file) in [
        (FontFamily::Proportional, "segoeui.ttf"),
        (FontFamily::Monospace, "consola.ttf"),
        (FontFamily::Name("semibold".into()), "seguisb.ttf"),
    ] {
        let list = fonts
            .families
            .entry(family)
            .or_insert_with(|| fallback.clone());
        if let Some(data) = system_font(file) {
            fonts.font_data.insert(file.to_string(), data);
            list.insert(0, file.to_string());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = WINDOW;
    visuals.window_fill = RAISED;
    visuals.window_stroke = stroke(1.0, HAIRLINE);
    visuals.window_rounding = Rounding::same(10.0);
    visuals.menu_rounding = Rounding::same(8.0);
    visuals.extreme_bg_color = WINDOW;
    visuals.widgets.noninteractive.bg_fill = RAISED;
    visuals.widgets.noninteractive.bg_stroke = stroke(1.0, HAIRLINE);
    visuals.widgets.inactive.bg_fill = RAISED;
    visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    visuals.widgets.hovered.bg_fill = HAIRLINE;
    visuals.widgets.hovered.weak_bg_fill = HAIRLINE;
    visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    visuals.widgets.active.bg_fill = HAIRLINE;
    visuals.widgets.active.weak_bg_fill = HAIRLINE;
    visuals.widgets.active.bg_stroke = Stroke::NONE;
    visuals.selection.bg_fill = AMBER.gamma_multiply(0.3);
    visuals.selection.stroke = stroke(1.0, AMBER);

    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.menu_margin = Margin::same(6.0);
    style.spacing.scroll = ScrollStyle::floating();
    ctx.set_style(style);
}
