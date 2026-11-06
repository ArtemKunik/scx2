//! USB Gain: the amp's USB recording level, in the live buffer's `0x0D` misc
//! block at offset 16.
//!
//! FUSE puts this knob in the advanced amp panel. It is a device setting, not
//! part of a `.fuse` preset, so it has its own dialog: the current value is
//! read from the misc block, the new level is written as one block frame plus
//! an execute frame, and the misc block is read back afterwards.

use std::time::Instant;

use eframe::egui::{self, Align, Color32, Layout, RichText};

use super::theme::*;
use super::worker::{percent, Cmd};
use super::App;
use crate::protocol::{self, USB_GAIN_OFFSET};

#[derive(Clone)]
pub(crate) struct UsbGainDialog {
    value: u8,
    /// Once the person moves the slider, a refresh must not pull it back.
    touched: bool,
}

impl App {
    pub(super) fn open_usb_gain_dialog(&mut self) {
        if self.amp_busy() || self.dialog_open() {
            self.status = "Finish the open dialog or wait for the amp first.".into();
            return;
        }
        let value = self.dump.usb_gain().unwrap_or(0);
        self.usb_gain = Some(UsbGainDialog {
            value,
            touched: false,
        });
        // Read the misc block again before offering to replace its byte.
        if self.connected {
            if let Some(tx) = &self.tx {
                let _ = tx.send(Cmd::Refresh);
            }
        }
    }

    pub(super) fn usb_gain_window(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.usb_gain.clone() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;

        let current = self.dump.usb_gain();
        let has_misc = self.dump.live_misc().is_some();
        // Until the slider is moved, keep it on the value the amp reports.
        if !dialog.touched {
            if let Some(raw) = current {
                dialog.value = raw;
            }
        }

        egui::Window::new("USB Gain")
            .id(egui::Id::new("usb-gain"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(
                    RichText::new(
                        "The amp's USB recording level. A device setting, not part of a .fuse preset.",
                    )
                    .font(regular(11.5))
                    .color(TEXT_3),
                );
                ui.add_space(8.0);
                match current {
                    Some(raw) => ui.label(
                        RichText::new(format!("Now: {raw} / 255 ({}%)", percent(raw)))
                            .font(regular(12.5))
                            .color(TEXT),
                    ),
                    None => ui.label(
                        RichText::new("Now: the amp has not reported its misc block")
                            .font(regular(12.5))
                            .color(ERROR),
                    ),
                };
                ui.add_space(10.0);

                ui.label(RichText::new("New level").font(semibold(12.0)).color(TEXT));
                if ui
                    .add(egui::Slider::new(&mut dialog.value, 0..=255).show_value(true))
                    .changed()
                {
                    dialog.touched = true;
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} / 255", dialog.value))
                            .font(regular(11.5))
                            .color(TEXT_2),
                    );
                    ui.label(
                        RichText::new(format!("{}%", percent(dialog.value)))
                            .font(regular(11.5))
                            .color(TEXT_3),
                    );
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "FUSE labels the range 0–255; the percentage is the same value on a 0–100% scale.",
                    )
                    .font(regular(11.0))
                    .color(TEXT_3),
                );

                if !has_misc {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Without the misc block the frame cannot preserve the other settings.",
                        )
                        .font(regular(11.0))
                        .color(ERROR),
                    );
                } else if !self.connected {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Not connected — the frame is printed to the status bar instead.",
                        )
                        .font(regular(11.5))
                        .color(TEXT_2),
                    );
                }
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ready = has_misc;
                    let label = if self.connected {
                        "Write USB Gain"
                    } else {
                        "Dry run"
                    };
                    let go = egui::Button::new(
                        RichText::new(label)
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
            if self.commit_usb_gain(&dialog) {
                self.usb_gain = None;
            } else {
                self.usb_gain = Some(dialog);
            }
            return;
        }
        if cancel || !open {
            self.usb_gain = None;
            return;
        }
        self.usb_gain = Some(dialog);
    }

    /// Send the misc-block frame, or print it when there is no amp.
    fn commit_usb_gain(&mut self, dialog: &UsbGainDialog) -> bool {
        let Some(misc) = self.dump.live_misc() else {
            self.error = Some("The amp has not reported its misc block.".into());
            return false;
        };
        let mut bytes = *misc.as_bytes();
        bytes[USB_GAIN_OFFSET] = dialog.value;
        let frame = protocol::as_host_frame(&bytes, protocol::HOST_CONTROL);
        if !self.connected {
            self.status = format!(
                "Dry run: {} + {} — connect the amp to write it",
                protocol::hex(&frame),
                protocol::hex(&protocol::execute_model_change())
            );
            return true;
        }
        let Some(tx) = self.tx.as_ref() else {
            self.error = Some("The amp worker is not running.".into());
            return false;
        };
        if let Err(error) = tx.send(Cmd::UsbGain {
            value: dialog.value,
            frame,
        }) {
            self.error = Some(format!("Could not reach the amp worker: {error}"));
            return false;
        }
        self.waiting = Some((Instant::now(), "USB gain"));
        self.status = format!(
            "Writing USB gain {} ({}%)…",
            dialog.value,
            percent(dialog.value)
        );
        true
    }

    /// The worker finished; the misc block has been compared.
    pub(super) fn on_usb_gain_done(&mut self, outcome: Result<String, String>) {
        self.waiting = None;
        self.usb_gain = None;
        match outcome {
            Ok(summary) => self.status = summary,
            Err(message) => self.error = Some(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Dump, Kind, AMP_TO_HOST, FRAME_MARKER, REPORT_SIZE};

    fn misc_report(value: u8) -> [u8; REPORT_SIZE] {
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = Kind::Misc.code();
        raw[3] = protocol::BANK_LIVE;
        raw[4] = 0x10;
        raw[6] = 1;
        raw[USB_GAIN_OFFSET] = value;
        raw
    }

    fn test_app(ctx: &egui::Context, gain: Option<u8>) -> App {
        super::super::theme::install(ctx);
        let mut app = App::fresh(ctx.clone());
        let mut dump = Dump::new();
        if let Some(value) = gain {
            dump.add_raw(&misc_report(value));
        }
        app.dump = dump;
        app.original = app.dump.clone();
        app.synced = true;
        app
    }

    #[test]
    fn the_dialog_opens_on_the_gain_the_amp_reports() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, Some(128));
        app.open_usb_gain_dialog();
        let dialog = app.usb_gain.clone().expect("the dialog is open");
        assert_eq!(dialog.value, 128);
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.usb_gain_window(ctx));
        }
        assert!(app.usb_gain.is_some());
    }

    #[test]
    fn a_dry_run_prints_the_misc_frame_and_writes_nothing() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, Some(64));
        app.connected = false;
        app.open_usb_gain_dialog();
        let mut dialog = app.usb_gain.clone().expect("the dialog is open");
        dialog.value = 200;
        assert!(app.commit_usb_gain(&dialog));
        assert!(app.status.starts_with("Dry run: "), "{}", app.status);
        assert!(app.status.contains("1c c3 0d"), "{}", app.status);
        // The dry run did not touch the dump.
        assert_eq!(app.dump.usb_gain(), Some(64));
    }

    #[test]
    fn without_a_misc_block_there_is_nothing_to_write() {
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, None);
        app.connected = false;
        app.open_usb_gain_dialog();
        let dialog = app.usb_gain.clone().expect("the dialog is open");
        assert!(!app.commit_usb_gain(&dialog));
        assert!(app.error.is_some());
    }
}
