//! Preset names: the 32-byte field the amp stores, and the input that edits it.
//!
//! A name is bytes, not characters. The amp has room for 32 of them, so a
//! longer name is refused rather than cut off behind the person's back. The
//! save and rename dialogs share this field so they behave the same way.

use eframe::egui::{self, RichText};

use super::theme::*;
use crate::protocol::NAME_LEN;

/// True when a name fits the amp's 32-byte field.
pub(crate) fn fits(name: &str) -> bool {
    name.len() <= NAME_LEN
}

/// Cut a name to the 32 bytes the amp stores, on a character boundary.
pub(crate) fn fit_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if out.len() + ch.len_utf8() > NAME_LEN {
            break;
        }
        out.push(ch);
    }
    out
}

/// Draw the name field with its byte counter and the Trim action.
///
/// Returns true when the text changed this frame, so a caller can stop
/// following the slot's stored name once the person starts typing.
pub(super) fn name_input(ui: &mut egui::Ui, name: &mut String) -> bool {
    ui.label(RichText::new("Name").font(semibold(12.0)).color(TEXT));
    ui.add_space(2.0);
    let edit = ui.add(
        egui::TextEdit::singleline(name)
            .font(regular(13.0))
            .desired_width(ui.available_width())
            .hint_text("Preset name"),
    );
    let mut changed = edit.changed();
    let bytes = name.len();
    ui.horizontal(|ui| {
        let over = bytes > NAME_LEN;
        ui.label(
            RichText::new(format!("{bytes} / {NAME_LEN} bytes"))
                .font(regular(11.0))
                .color(if over { ERROR } else { TEXT_3 }),
        );
        if over {
            ui.label(
                RichText::new("— the amp would cut this off")
                    .font(regular(11.0))
                    .color(ERROR),
            );
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(format!("Trim to {NAME_LEN}")).font(regular(11.0)),
                    )
                    .small(),
                )
                .clicked()
            {
                *name = fit_name(name);
                changed = true;
            }
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_is_cut_to_32_bytes_not_silently_dropped() {
        assert!(!fits(&"x".repeat(33)));
        assert_eq!(fit_name(&"x".repeat(40)).len(), NAME_LEN);
    }

    #[test]
    fn a_name_is_never_cut_mid_character() {
        let name = format!("{}{}", "a".repeat(31), "é");
        let fitted = fit_name(&name);
        assert_eq!(fitted.len(), 31);
        assert!(fitted.is_char_boundary(fitted.len()));
    }
}
