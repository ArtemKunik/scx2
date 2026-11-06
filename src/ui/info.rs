//! "Preset Info…": FUSE's Preset Info dialog, stored beside the device bytes.
//!
//! Description, tags, genres, author, rating and comments live in a backup's
//! `info` object. They are never written to the amp and never change a block,
//! so a JSON backup keeps them and a `.fuse` export keeps only the device name.

use eframe::egui::{self, Align, Layout, RichText};

use super::theme::*;
use super::App;
use crate::protocol::{PresetInfo, BANK_AMP, BANK_FX};

#[derive(Clone)]
pub(crate) struct PresetInfoDialog {
    bank: u8,
    index: u8,
    info: PresetInfo,
    /// Tags are edited as one comma-separated line.
    tags: String,
}

impl App {
    /// Open the dialog for a stored preset.
    pub(super) fn open_preset_info(&mut self, bank: u8, index: u8) {
        if bank != BANK_AMP && bank != BANK_FX {
            return;
        }
        if self.dialog_open() {
            self.status = "Finish the open dialog first.".into();
            return;
        }
        let Some(preset) = self.dump.get(bank, index) else {
            return;
        };
        self.info = Some(PresetInfoDialog {
            bank,
            index,
            info: preset.info.clone(),
            tags: preset.info.tags.join(", "),
        });
    }

    pub(super) fn info_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.info.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;

        let header = match self.dump.get(dialog.bank, dialog.index) {
            Some(preset) => format!(
                "{} {} · {:?}",
                bank_title(dialog.bank),
                dialog.index,
                if preset.name.is_empty() {
                    "(no device name)"
                } else {
                    preset.name.as_str()
                }
            ),
            None => format!("{} {}", bank_title(dialog.bank), dialog.index),
        };

        egui::Window::new("Preset Info")
            .id(egui::Id::new("preset-info"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(470.0);
                ui.label(
                    RichText::new(header)
                        .font(regular(12.0))
                        .color(TEXT),
                );
                ui.label(
                    RichText::new(
                        "Saved beside the device bytes; the amp never sees it. Save As writes it to a backup.",
                    )
                    .font(regular(11.0))
                    .color(TEXT_3),
                );
                ui.add_space(10.0);

                egui::Grid::new("preset-info-fields")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Description").font(semibold(12.0)));
                        ui.add(
                            egui::TextEdit::multiline(&mut dialog.info.description)
                                .desired_rows(2)
                                .desired_width(340.0),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Author").font(semibold(12.0)));
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.info.author)
                                .desired_width(340.0),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Tags").font(semibold(12.0)));
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.tags)
                                .hint_text("comma separated")
                                .desired_width(340.0),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Genre").font(semibold(12.0)));
                        ui.add(
                            egui::TextEdit::singleline(&mut dialog.info.genre)
                                .desired_width(340.0),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Sub-genres").font(semibold(12.0)));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut dialog.info.sub_genres[0])
                                    .hint_text("main")
                                    .desired_width(165.0),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut dialog.info.sub_genres[1])
                                    .hint_text("second")
                                    .desired_width(165.0),
                            );
                        });
                        ui.end_row();

                        ui.label(RichText::new("Rating").font(semibold(12.0)));
                        ui.horizontal(|ui| {
                            ui.add(egui::Slider::new(&mut dialog.info.rating, 0..=5));
                            ui.label(
                                RichText::new(if dialog.info.rating == 0 {
                                    "unrated"
                                } else {
                                    "stars"
                                })
                                .font(regular(11.0))
                                .color(TEXT_3),
                            );
                        });
                        ui.end_row();

                        ui.label(RichText::new("Comments").font(semibold(12.0)));
                        ui.add(
                            egui::TextEdit::multiline(&mut dialog.info.comments)
                                .desired_rows(2)
                                .desired_width(340.0),
                        );
                        ui.end_row();
                    });

                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "The amp's preset list only carries the 32-byte name; everything here is local library data.",
                    )
                    .font(regular(11.0))
                    .color(TEXT_3),
                );

                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let save = egui::Button::new(
                        RichText::new("Save Info")
                            .font(semibold(13.0))
                            .color(AMBER_INK),
                    )
                    .fill(AMBER);
                    if ui.add(save).clicked() {
                        confirm = true;
                    }
                    if ui
                        .add(egui::Button::new(
                            RichText::new("Cancel").font(regular(12.5)),
                        ))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });

        if confirm {
            if self.save_preset_info(&dialog) {
                self.info = None;
            } else {
                self.info = Some(dialog);
            }
            return;
        }
        if cancel || !open {
            self.info = None;
            return;
        }
        self.info = Some(dialog);
    }

    /// Apply the dialog to the preset. Returns false to keep the dialog open.
    fn save_preset_info(&mut self, dialog: &PresetInfoDialog) -> bool {
        let mut info = dialog.info.clone();
        info.description = info.description.trim().to_string();
        info.author = info.author.trim().to_string();
        info.genre = info.genre.trim().to_string();
        info.sub_genres = info.sub_genres.map(|name| name.trim().to_string());
        info.comments = info.comments.trim().to_string();
        info.tags = dialog
            .tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect();

        if self
            .dump
            .get(dialog.bank, dialog.index)
            .is_some_and(|preset| preset.info == info)
        {
            self.status = "Preset info unchanged".into();
            return true;
        }
        self.begin_edit();
        let Some(preset) = self.dump.get_mut(dialog.bank, dialog.index) else {
            self.error = Some("That preset is no longer in the dump.".into());
            return false;
        };
        preset.info = info;
        self.status = "Preset info updated · Save As keeps it in the backup".into();
        true
    }
}

fn bank_title(bank: u8) -> &'static str {
    match bank {
        BANK_FX => "FX preset",
        _ => "Amp preset",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        Dump, Kind, Message, Preset, AMP_TO_HOST, FRAME_MARKER, OFF_MODEL, REPORT_SIZE,
    };

    fn test_app(ctx: &egui::Context) -> App {
        super::super::theme::install(ctx);
        let mut app = App::fresh(ctx.clone());
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = Kind::Amp.code();
        raw[3] = BANK_AMP;
        raw[4] = 3;
        raw[6] = 1;
        raw[OFF_MODEL] = 0x64;
        let mut preset = Preset::new(BANK_AMP, 3);
        preset.set_name("Tweed Bassman");
        preset.add(Message::from_bytes(raw).expect("a valid amp block"));
        let mut dump = Dump::new();
        dump.insert(preset);
        app.dump = dump;
        app.original = app.dump.clone();
        app.synced = true;
        app
    }

    #[test]
    fn the_info_dialog_opens_on_a_stored_preset() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        app.open_preset_info(BANK_AMP, 3);
        assert!(app.info.is_some());
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.info_window(ctx));
        }
        assert!(app.info.is_some(), "the dialog stays open while idle");
    }

    #[test]
    fn info_edits_stay_local_and_mark_the_dump_dirty() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        assert!(!app.dirty());
        app.open_preset_info(BANK_AMP, 3);
        let mut dialog = app.info.clone().expect("the dialog is open");
        dialog.info.description = "  Edges of breakup  ".into();
        dialog.info.author = "  A. Player ".into();
        dialog.tags = " blues, , clean ".into();
        dialog.info.rating = 4;
        assert!(app.save_preset_info(&dialog));

        let preset = app.dump.get(BANK_AMP, 3).expect("the preset");
        assert_eq!(preset.info.description, "Edges of breakup");
        assert_eq!(preset.info.author, "A. Player");
        assert_eq!(preset.info.tags, vec!["blues", "clean"]);
        assert!(app.dirty(), "an unsaved info edit is an unsaved change");
        // Device bytes did not move.
        assert!(app.pending().is_empty());
    }

    #[test]
    fn info_survives_save_as_and_reopen() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        app.open_preset_info(BANK_AMP, 3);
        let mut dialog = app.info.clone().expect("the dialog is open");
        dialog.info.description = "Saved once".into();
        dialog.info.author = "A. Player".into();
        dialog.tags = "blues, clean".into();
        assert!(app.save_preset_info(&dialog));

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("scx2-info-{stamp}.json"));
        app.save_as(&path);
        assert!(!app.dirty(), "saving the backup records the info");

        app.load_file(&path);
        let preset = app.dump.get(BANK_AMP, 3).expect("the preset");
        assert_eq!(preset.info.description, "Saved once");
        assert_eq!(preset.info.author, "A. Player");
        assert_eq!(preset.info.tags, vec!["blues", "clean"]);
        assert!(!app.dirty());
        let _ = std::fs::remove_file(path);
    }
}
