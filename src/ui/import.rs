//! "Import…": copy selected modules from another preset into the live buffer.
//!
//! FUSE's IMPORT dialog takes the amp, stomp, modulation, delay and reverb
//! blocks from another preset and leaves everything else where it was. The
//! source can be a backup (JSON) or a single FUSE `.fuse` preset. Imported
//! blocks are retargeted to the live buffer; the source file is never written.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Color32, Layout, RichText};

use super::theme::*;
use super::App;
use crate::models;
use crate::protocol::{self, Dump, Kind, Message, Preset, BANK_LIVE};

/// The modules FUSE's SCX2 IMPORT dialog offers. USB Gain is an amp setting,
/// not a preset block, so it is not part of this dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Modules {
    pub(crate) amp: bool,
    pub(crate) stomp: bool,
    pub(crate) modulation: bool,
    pub(crate) delay: bool,
    pub(crate) reverb: bool,
}

impl Modules {
    fn covers(self, kind: Kind) -> bool {
        match kind {
            Kind::Amp => self.amp,
            Kind::Stomp => self.stomp,
            Kind::Mod => self.modulation,
            Kind::Delay => self.delay,
            Kind::Reverb => self.reverb,
            _ => false,
        }
    }

    fn any(self) -> bool {
        self.amp || self.stomp || self.modulation || self.delay || self.reverb
    }
}

pub(crate) struct ImportDialog {
    path: PathBuf,
    device: String,
    source: Dump,
    key: (u8, u8),
    modules: Modules,
}

impl App {
    /// Pick a preset file and open the module picker.
    pub(super) fn open_import_dialog(&mut self) {
        if self.amp_busy()
            || self.store.is_some()
            || self.rename.is_some()
            || self.restore_dialog_open()
        {
            self.status = "Finish the open dialog or wait for the amp first.".into();
            return;
        }
        if self.dump.get(BANK_LIVE, 0).is_none() {
            self.error = Some(
                "There is no live buffer to import into. Connect the amp or open a backup.".into(),
            );
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Fender FUSE preset", &["fuse"])
            .add_filter("SCX2 backup", &["json"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(format!("Could not read {}: {error}", path.display()));
                return;
            }
        };
        let is_fuse = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("fuse"));
        let (source, device) = if is_fuse {
            match crate::fuse::parse(&text) {
                Ok(preset) => (Dump::single(preset), "Fender FUSE preset".to_string()),
                Err(error) => {
                    self.error = Some(format!(
                        "{} is not a FUSE preset: {error:#}",
                        path.display()
                    ));
                    return;
                }
            }
        } else {
            let device = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|value| {
                    value
                        .get("device")
                        .and_then(|device| device.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "SCX2 backup".into());
            match Dump::from_json(&text) {
                Ok(dump) => (dump, device),
                Err(error) => {
                    self.error = Some(format!("{} is not an SCX2 backup: {error}", path.display()));
                    return;
                }
            }
        };
        let Some(key) = first_preset(&source) else {
            self.error = Some(format!("{} has no presets.", path.display()));
            return;
        };
        let modules = source
            .get(key.0, key.1)
            .map(default_modules)
            .unwrap_or_default();
        self.import = Some(ImportDialog {
            path,
            device,
            source,
            key,
            modules,
        });
    }

    pub(super) fn import_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.import.take() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        let mut source_changed = false;

        let Some(source) = dialog.source.get(dialog.key.0, dialog.key.1) else {
            self.error = Some("Choose a source preset.".into());
            return;
        };
        let live = self.dump.get(BANK_LIVE, 0).cloned();
        let source_label = key_label(&dialog.source, dialog.key);

        egui::Window::new("Import Modules")
            .id(egui::Id::new("import-modules"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(
                    RichText::new(format!("{} · {}", dialog.device, file_name(&dialog.path)))
                        .font(regular(11.5))
                        .color(TEXT_3),
                );
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Source preset").font(semibold(12.0)).color(TEXT));
                    egui::ComboBox::from_id_source("import-source")
                        .selected_text(source_label.clone())
                        .show_ui(ui, |ui| {
                            for preset in dialog.source.presets() {
                                let key = (preset.bank, preset.index);
                                if ui
                                    .selectable_value(
                                        &mut dialog.key,
                                        key,
                                        key_label(&dialog.source, key),
                                    )
                                    .clicked()
                                {
                                    source_changed = true;
                                }
                            }
                        });
                });
                // A new source starts from its own modules again.
                if source_changed {
                    if let Some(preset) = dialog.source.get(dialog.key.0, dialog.key.1) {
                        dialog.modules = default_modules(preset);
                    }
                }
                let source = dialog.source.get(dialog.key.0, dialog.key.1).unwrap_or(source);
                ui.label(
                    RichText::new(match &live {
                        Some(preset) => format!(
                            "Into the live buffer · now {}",
                            model_label(Kind::Amp, Some(preset))
                        ),
                        None => "Into the live buffer".to_string(),
                    })
                    .font(regular(11.5))
                    .color(TEXT_2),
                );

                if !dialog.device.contains("Super Champ X2")
                    && dialog.device != "Fender FUSE preset"
                    && dialog.device != "SCX2 backup"
                {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "This file came from {}; models the SCX2 does not have will be copied as unknown ids.",
                            dialog.device
                        ))
                        .font(regular(11.0))
                        .color(ERROR),
                    );
                }
                ui.add_space(10.0);

                ui.label(RichText::new("Modules").font(semibold(12.0)).color(TEXT));
                ui.add_space(2.0);
                module_row(
                    ui,
                    &mut dialog.modules.amp,
                    "Amplifier",
                    Kind::Amp,
                    source,
                    live.as_ref(),
                );
                module_row(
                    ui,
                    &mut dialog.modules.stomp,
                    "Stomp",
                    Kind::Stomp,
                    source,
                    live.as_ref(),
                );
                module_row(
                    ui,
                    &mut dialog.modules.modulation,
                    "Modulation",
                    Kind::Mod,
                    source,
                    live.as_ref(),
                );
                module_row(
                    ui,
                    &mut dialog.modules.delay,
                    "Delay",
                    Kind::Delay,
                    source,
                    live.as_ref(),
                );
                module_row(
                    ui,
                    &mut dialog.modules.reverb,
                    "Reverb",
                    Kind::Reverb,
                    source,
                    live.as_ref(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Only ticked modules are written; every other block stays byte-identical. A ticked effect that the source does not have is cleared. USB Gain is not part of a preset and is not imported.",
                    )
                    .font(regular(11.0))
                    .color(TEXT_3),
                );

                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ready = dialog.modules.any() && live.is_some();
                    let go = egui::Button::new(
                        RichText::new("Import")
                            .font(semibold(13.0))
                            .color(if ready { AMBER_INK } else { TEXT_3 }),
                    )
                    .fill(if ready {
                        AMBER
                    } else {
                        Color32::from_white_alpha(8)
                    });
                    if ui.add_enabled(ready, go).clicked() {
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
            let Some(preset) = dialog.source.get(dialog.key.0, dialog.key.1) else {
                self.error = Some("Choose a source preset.".into());
                self.import = Some(dialog);
                return;
            };
            let label = key_label(&dialog.source, dialog.key);
            if self.apply_import(preset, dialog.modules, &label) {
                self.import = None;
            } else {
                self.import = Some(dialog);
            }
            return;
        }
        if cancel || !open {
            self.import = None;
            return;
        }
        self.import = Some(dialog);
    }

    /// Replace the ticked modules in the live buffer. Returns false when the
    /// import did not happen and the dialog should stay open.
    fn apply_import(&mut self, source: &Preset, modules: Modules, label: &str) -> bool {
        if !modules.any() {
            self.error = Some("Tick at least one module to import.".into());
            return false;
        }
        if self.dump.get(BANK_LIVE, 0).is_none() {
            self.error = Some("There is no live buffer to import into.".into());
            return false;
        }
        // One undo step for the whole import.
        self.begin_edit();
        let applied = {
            let live = self
                .dump
                .get_mut(BANK_LIVE, 0)
                .expect("the live buffer was checked above");
            apply_modules(live, source, modules)
        };
        if applied.is_empty() {
            self.error = Some("The source has none of the ticked modules.".into());
            return false;
        }
        self.selected = (BANK_LIVE, 0);
        self.status = format!("Imported {} from {label}", applied.join(", "));
        true
    }
}

/// Copy the ticked blocks into `live`, retargeted to the live buffer.
///
/// Returns the names of the modules that were applied. Unselected blocks are
/// not touched; a ticked effect the source does not have is cleared.
pub(crate) fn apply_modules(
    live: &mut Preset,
    source: &Preset,
    modules: Modules,
) -> Vec<&'static str> {
    let mut applied = Vec::new();
    for kind in [Kind::Amp, Kind::Stomp, Kind::Mod, Kind::Delay, Kind::Reverb] {
        if !modules.covers(kind) {
            continue;
        }
        match source.block(kind) {
            Some(message) => {
                // The source block belongs to its own bank/slot; the live
                // buffer is bank 0, so writing it as-is would target the wrong
                // memory.
                let bytes = protocol::retarget(message.as_bytes(), BANK_LIVE, 0);
                let Some(copied) = Message::from_bytes(bytes) else {
                    continue;
                };
                live.blocks.insert(kind.code(), copied);
                applied.push(kind.label());
            }
            // An amp block always exists in a valid preset; only effects are
            // cleared when the source has none.
            None if kind != Kind::Amp && live.block(kind).is_some() => {
                live.load_model(kind, 0);
                applied.push(kind.label());
            }
            None => {}
        }
    }
    applied
}

/// Which modules the source preset has, with only known models ticked.
fn default_modules(source: &Preset) -> Modules {
    Modules {
        amp: known_model(Kind::Amp, source),
        stomp: known_model(Kind::Stomp, source),
        modulation: known_model(Kind::Mod, source),
        delay: known_model(Kind::Delay, source),
        reverb: known_model(Kind::Reverb, source),
    }
}

fn known_model(kind: Kind, preset: &Preset) -> bool {
    preset
        .block(kind)
        .and_then(Message::model)
        .filter(|model| *model != 0)
        .is_some_and(|model| models::model_name(kind, model).is_some())
}

fn model_label(kind: Kind, preset: Option<&Preset>) -> String {
    match preset
        .and_then(|preset| preset.block(kind))
        .and_then(Message::model)
    {
        Some(model) if model != 0 => match models::model_name(kind, model) {
            Some(name) => name.to_string(),
            None => format!("unknown 0x{model:04x}"),
        },
        _ => "none".to_string(),
    }
}

fn module_row(
    ui: &mut egui::Ui,
    on: &mut bool,
    title: &str,
    kind: Kind,
    source: &Preset,
    live: Option<&Preset>,
) {
    ui.horizontal(|ui| {
        ui.checkbox(on, RichText::new(title).font(regular(12.0)));
        ui.label(
            RichText::new(format!(
                "{} → {}",
                model_label(kind, Some(source)),
                model_label(kind, live)
            ))
            .font(regular(11.0))
            .color(TEXT_3),
        );
        if let Some(model) = source
            .block(kind)
            .and_then(Message::model)
            .filter(|model| *model != 0)
        {
            if models::model_name(kind, model).is_none() {
                ui.label(
                    RichText::new("unknown model — copied as-is if ticked")
                        .font(regular(10.5))
                        .color(ERROR),
                );
            }
        }
    });
}

fn first_preset(dump: &Dump) -> Option<(u8, u8)> {
    dump.get(BANK_LIVE, 0)
        .or_else(|| dump.presets().next())
        .map(|preset| (preset.bank, preset.index))
}

fn key_label(source: &Dump, key: (u8, u8)) -> String {
    match source.get(key.0, key.1) {
        Some(preset) => format!(
            "{} {} · {}",
            protocol::bank_label(key.0),
            key.1,
            if preset.name.is_empty() {
                "unnamed"
            } else {
                preset.name.as_str()
            }
        ),
        None => format!("{} {}", protocol::bank_label(key.0), key.1),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("preset")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AMP_TO_HOST, FRAME_MARKER, OFF_MODEL, OFF_PARAMS, REPORT_SIZE};

    fn report(kind: Kind, bank: u8, index: u8, model: u16, param: u8) -> Message {
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = kind.code();
        raw[3] = bank;
        raw[4] = index;
        raw[6] = 1;
        let [low, high] = model.to_le_bytes();
        raw[OFF_MODEL] = low;
        raw[OFF_MODEL + 1] = high;
        raw[OFF_PARAMS] = param;
        Message::from_bytes(raw).expect("a valid block")
    }

    fn live_preset() -> Preset {
        let mut preset = Preset::new(BANK_LIVE, 0);
        for (kind, model, param) in [
            (Kind::Amp, 0x00eb, 0x10),
            (Kind::Stomp, 0x0007, 0x11),
            (Kind::Mod, 0x0012, 0x12),
            (Kind::Delay, 0x002b, 0x13),
            (Kind::Reverb, 0x0024, 0x14),
        ] {
            preset
                .blocks
                .insert(kind.code(), report(kind, BANK_LIVE, 0, model, param));
        }
        preset
    }

    fn source_preset() -> Preset {
        let mut preset = Preset::new(protocol::BANK_AMP, 3);
        preset.name = "source".into();
        for (kind, model, param) in [
            (Kind::Amp, 0x0064, 0x20),
            (Kind::Stomp, 0x0007, 0x21),
            (Kind::Mod, 0x0012, 0x22),
            (Kind::Delay, 0x002b, 0x23),
            (Kind::Reverb, 0x003a, 0x24),
        ] {
            preset.blocks.insert(
                kind.code(),
                report(kind, protocol::BANK_AMP, 3, model, param),
            );
        }
        preset
    }

    #[test]
    fn importing_only_the_reverb_leaves_every_other_block_identical() {
        let mut live = live_preset();
        let before = live.clone();
        let applied = apply_modules(
            &mut live,
            &source_preset(),
            Modules {
                reverb: true,
                ..Modules::default()
            },
        );
        assert_eq!(applied, vec!["reverb"]);
        for kind in [Kind::Amp, Kind::Stomp, Kind::Mod, Kind::Delay] {
            assert_eq!(
                live.block(kind),
                before.block(kind),
                "{kind:?} must not move"
            );
        }
        let reverb = live.block(Kind::Reverb).expect("reverb was imported");
        assert_eq!(reverb.model(), Some(0x003a));
        assert_eq!(reverb.byte(OFF_PARAMS), 0x24);
    }

    #[test]
    fn an_imported_block_is_retargeted_to_the_live_buffer() {
        let mut live = live_preset();
        apply_modules(
            &mut live,
            &source_preset(),
            Modules {
                reverb: true,
                ..Modules::default()
            },
        );
        let block = live.block(Kind::Reverb).expect("reverb was imported");
        assert_eq!((block.bank(), block.index()), (BANK_LIVE, 0));
        assert_eq!(block.model(), Some(0x003a));
    }

    #[test]
    fn a_ticked_effect_missing_from_the_source_is_cleared() {
        let mut source = source_preset();
        source.blocks.remove(&Kind::Stomp.code());
        let mut live = live_preset();
        let applied = apply_modules(
            &mut live,
            &source,
            Modules {
                stomp: true,
                ..Modules::default()
            },
        );
        assert_eq!(applied, vec!["stomp"]);
        let block = live.block(Kind::Stomp).expect("the dest keeps its block");
        assert_eq!(block.model(), Some(0));
        assert!(block.params().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn unticked_modules_are_never_touched() {
        let mut live = live_preset();
        let before = live.clone();
        assert!(apply_modules(&mut live, &source_preset(), Modules::default()).is_empty());
        for kind in [Kind::Amp, Kind::Stomp, Kind::Mod, Kind::Delay, Kind::Reverb] {
            assert_eq!(live.block(kind), before.block(kind));
        }
    }

    #[test]
    fn an_unknown_model_is_copied_when_ticked() {
        let mut source = source_preset();
        source.blocks.insert(
            Kind::Reverb.code(),
            report(Kind::Reverb, protocol::BANK_AMP, 3, 0x9999, 0x77),
        );
        let mut live = live_preset();
        apply_modules(
            &mut live,
            &source,
            Modules {
                reverb: true,
                ..Modules::default()
            },
        );
        let block = live.block(Kind::Reverb).expect("reverb was imported");
        assert_eq!(block.model(), Some(0x9999));
        assert_eq!(block.byte(OFF_PARAMS), 0x77);
    }

    #[test]
    fn the_default_modules_tick_known_models_only() {
        let mut source = source_preset();
        assert_eq!(
            default_modules(&source),
            Modules {
                amp: true,
                stomp: true,
                modulation: true,
                delay: true,
                reverb: true,
            }
        );
        source.blocks.insert(
            Kind::Reverb.code(),
            report(Kind::Reverb, protocol::BANK_AMP, 3, 0x9999, 0x77),
        );
        assert!(!default_modules(&source).reverb);
    }

    fn test_app(ctx: &egui::Context) -> App {
        super::super::theme::install(ctx);
        let mut app = App::fresh(ctx.clone());
        let mut dump = Dump::new();
        dump.insert(live_preset());
        app.dump = dump;
        app.original = app.dump.clone();
        app.synced = true;
        app
    }

    #[test]
    fn the_import_dialog_renders_with_module_rows() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        let mut source = Dump::new();
        source.insert(source_preset());
        app.import = Some(ImportDialog {
            path: PathBuf::from("tweed.fuse"),
            device: "Fender FUSE preset".into(),
            source,
            key: (protocol::BANK_AMP, 3),
            modules: Modules::default(),
        });
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.import_window(ctx));
        }
        assert!(app.import.is_some(), "the dialog stays open while idle");
    }

    #[test]
    fn applying_an_import_updates_the_live_buffer_and_selects_it() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx);
        let source = source_preset();
        assert!(app.apply_import(
            &source,
            Modules {
                reverb: true,
                ..Modules::default()
            },
            "amp 3",
        ));
        assert_eq!(app.selected, (BANK_LIVE, 0));
        let live = app.dump.get(BANK_LIVE, 0).expect("the live buffer");
        assert_eq!(
            live.block(Kind::Reverb).map(Message::model),
            Some(Some(0x003a))
        );
        assert_eq!(
            live.block(Kind::Stomp).map(Message::model),
            Some(Some(0x0007))
        );
        assert!(app.status.contains("reverb"), "{}", app.status);
    }
}
