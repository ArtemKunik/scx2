//! The MIDI foot-controller dialog and the bridge from MIDI to the amp.

use std::collections::BTreeMap;
use std::time::Instant;

use eframe::egui::{self, RichText};

use super::app::PendingLive;
use super::theme::*;
use super::App;
use crate::midi as midi_input;
use crate::protocol::{self, BANK_LIVE};

impl App {
    pub(super) fn open_midi_window(&mut self) {
        self.show_midi = true;
        self.refresh_midi_ports();
    }

    pub(super) fn refresh_midi_ports(&mut self) {
        match midi_input::ports() {
            Ok(mut ports) => {
                if let Some(port) = &self.midi.port {
                    if !ports.contains(port) {
                        ports.push(port.clone());
                    }
                }
                self.midi_ports = ports;
            }
            Err(error) => {
                self.midi_error = Some(format!("Could not list MIDI inputs: {error:#}"));
            }
        }
    }

    /// Close the old input and open the configured one, if any.
    pub(super) fn start_midi(&mut self) {
        self.midi_listener = None;
        self.midi_rx = None;
        if !self.midi.enabled {
            return;
        }
        let Some(port) = self.midi.port.clone() else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ctx.clone();
        match midi_input::connect(&port, move |event| {
            let _ = tx.send(event);
            // A footswitch press must not wait for the next animation frame.
            ctx.request_repaint();
        }) {
            Ok(listener) => {
                self.midi_listener = Some(listener);
                self.midi_rx = Some(rx);
                self.midi_error = None;
                self.status = format!("MIDI: listening on {port}");
            }
            Err(error) => {
                self.midi_error = Some(format!("{error:#}"));
            }
        }
    }

    fn save_midi(&mut self) {
        self.midi_router.set_config(self.midi.clone());
        if let Err(error) = self.midi.save() {
            self.midi_error = Some(format!("Could not save MIDI settings: {error:#}"));
        }
    }

    /// Take anything the controller sent since the last frame.
    pub(super) fn poll_midi(&mut self) {
        let events = {
            let Some(rx) = &self.midi_rx else {
                return;
            };
            let mut events = Vec::new();
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
            events
        };

        for event in events {
            if let Some(effect) = self.midi_learn {
                if self
                    .midi
                    .channel
                    .is_some_and(|channel| channel != event.channel())
                {
                    continue;
                }
                let trigger = event.trigger();
                let mode = self
                    .midi
                    .binding(effect)
                    .map(|binding| binding.mode)
                    .unwrap_or_default();
                self.midi
                    .set_binding(effect, midi_input::Binding { trigger, mode });
                self.midi_learn = None;
                self.midi_router.set_config(self.midi.clone());
                self.save_midi();
                self.status = format!("MIDI: {} is {}", effect.label(), trigger.label());
                continue;
            }

            let states = self.effect_states();
            if let Some(action) = self.midi_router.handle(event, &states) {
                self.apply_midi_action(action);
            }
        }
    }

    fn effect_states(&self) -> BTreeMap<midi_input::Effect, bool> {
        let live = self.dump.live();
        midi_input::EFFECTS
            .iter()
            .map(|effect| {
                let bypassed = live
                    .and_then(|preset| preset.block(effect.kind()))
                    .is_some_and(|block| block.byte(protocol::OFF_BYPASS) != 0);
                (*effect, bypassed)
            })
            .collect()
    }

    fn effect_loaded(&self) -> BTreeMap<midi_input::Effect, bool> {
        let live = self.dump.live();
        midi_input::EFFECTS
            .iter()
            .map(|effect| {
                let loaded = live
                    .and_then(|preset| preset.block(effect.kind()))
                    .and_then(|block| block.model())
                    .is_some_and(|model| model != 0);
                (*effect, loaded)
            })
            .collect()
    }

    /// Change the live buffer and let `send_live_edits` send the bypass frame.
    fn apply_midi_action(&mut self, action: midi_input::Action) {
        if !self.connected {
            self.status = format!("MIDI: {} — the amp is not connected", action.effect.label());
            return;
        }
        let kind = action.effect.kind();
        let Some(live) = self.dump.get_mut(BANK_LIVE, 0) else {
            return;
        };
        let Some(block) = live.block_mut(kind) else {
            self.status = format!(
                "MIDI: no {} block in the live buffer",
                action.effect.label()
            );
            return;
        };
        if block.model().unwrap_or(0) == 0 {
            self.status = format!("MIDI: no {} loaded", action.effect.label());
            return;
        }
        let value = u8::from(action.bypassed);
        block.set_byte(protocol::OFF_BYPASS, value);
        // A dump can overtake the write; keep the footswitch's value on screen
        // until the amp reports it back.
        self.pending_live.insert(
            (kind.code(), protocol::OFF_BYPASS),
            PendingLive {
                value,
                seen: Instant::now(),
            },
        );
        self.status = format!(
            "MIDI: {} {}",
            action.effect.label(),
            if action.bypassed { "off" } else { "on" }
        );
    }

    pub(super) fn midi_window(&mut self, ctx: &egui::Context) {
        if !self.show_midi {
            return;
        }

        let mut open = self.show_midi;
        let mut config = self.midi.clone();
        let states = self.effect_states();
        let loaded = self.effect_loaded();
        let ports = self.midi_ports.clone();
        let listening = self
            .midi_listener
            .as_ref()
            .map(|input| input.port().to_string());
        let error = self.midi_error.clone();
        let mut learn = self.midi_learn;
        let mut refresh = false;
        let mut reconnect = false;
        let mut clear = None;

        egui::Window::new("MIDI Foot Control")
            .id(egui::Id::new("midi-foot-control"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -24.0])
            .show(ctx, |ui| {
                ui.set_width(560.0);
                ui.label(
                    RichText::new(
                        "The amp has no MIDI input. scx2 listens to a controller here and sends \
                         the same bypass frame as the ON/OFF pill, so the amp stays connected \
                         to this computer.",
                    )
                    .font(regular(11.5))
                    .color(TEXT_3),
                );
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Input").font(semibold(12.0)).color(TEXT));
                    let selected = config
                        .port
                        .clone()
                        .unwrap_or_else(|| "No controller".into());
                    egui::ComboBox::from_id_source("midi-port")
                        .selected_text(selected)
                        .width(300.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(config.port.is_none(), "No controller")
                                .clicked()
                            {
                                config.port = None;
                                reconnect = true;
                            }
                            for port in &ports {
                                let selected = config.port.as_deref() == Some(port.as_str());
                                if ui.selectable_label(selected, port).clicked() {
                                    config.port = Some(port.clone());
                                    reconnect = true;
                                }
                            }
                        });
                    if ui.button("Refresh").clicked() {
                        refresh = true;
                    }
                });

                ui.add_space(4.0);
                if let Some(port) = &listening {
                    ui.label(
                        RichText::new(format!("Listening on {port}"))
                            .font(regular(11.0))
                            .color(AMBER),
                    );
                } else if let Some(error) = &error {
                    ui.label(RichText::new(error).font(regular(11.0)).color(ERROR));
                } else if config.port.is_some() {
                    ui.label(
                        RichText::new("Not listening")
                            .font(regular(11.0))
                            .color(TEXT_3),
                    );
                }

                ui.add_space(6.0);
                let mut enabled = config.enabled;
                if ui
                    .checkbox(
                        &mut enabled,
                        RichText::new("Listen for MIDI").font(regular(12.0)),
                    )
                    .changed()
                {
                    config.enabled = enabled;
                    reconnect = true;
                }

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Channel").font(semibold(12.0)).color(TEXT));
                    egui::ComboBox::from_id_source("midi-channel")
                        .selected_text(match config.channel {
                            Some(channel) => channel.to_string(),
                            None => "Any".into(),
                        })
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(config.channel.is_none(), "Any")
                                .clicked()
                            {
                                config.channel = None;
                            }
                            for channel in 1..=16u8 {
                                if ui
                                    .selectable_label(
                                        config.channel == Some(channel),
                                        channel.to_string(),
                                    )
                                    .clicked()
                                {
                                    config.channel = Some(channel);
                                }
                            }
                        });
                });

                ui.add_space(10.0);
                egui::Grid::new("midi-bindings")
                    .num_columns(5)
                    .striped(true)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Effect").font(semibold(11.5)).color(TEXT_2));
                        ui.label(RichText::new("State").font(semibold(11.5)).color(TEXT_2));
                        ui.label(RichText::new("Control").font(semibold(11.5)).color(TEXT_2));
                        ui.label(RichText::new("Mode").font(semibold(11.5)).color(TEXT_2));
                        ui.label("");
                        ui.end_row();

                        for effect in midi_input::EFFECTS {
                            let is_loaded = loaded.get(&effect).copied().unwrap_or(false);
                            let bypassed = states.get(&effect).copied().unwrap_or(false);
                            let binding = config.binding(effect);

                            ui.label(
                                RichText::new(effect.label())
                                    .font(semibold(12.0))
                                    .color(TEXT),
                            );
                            ui.label(
                                RichText::new(if !is_loaded {
                                    "empty"
                                } else if bypassed {
                                    "OFF"
                                } else {
                                    "ON"
                                })
                                .font(semibold(11.0))
                                .color(
                                    if is_loaded && !bypassed {
                                        AMBER
                                    } else {
                                        TEXT_3
                                    },
                                ),
                            );
                            ui.label(
                                RichText::new(match binding {
                                    Some(binding) => binding.trigger.label(),
                                    None => "Not set".into(),
                                })
                                .font(regular(11.5))
                                .color(if binding.is_some() { TEXT } else { TEXT_3 }),
                            );

                            let mut mode = binding.map(|binding| binding.mode).unwrap_or_default();
                            ui.add_enabled_ui(binding.is_some(), |ui| {
                                egui::ComboBox::from_id_source(("midi-mode", effect))
                                    .selected_text(mode.label())
                                    .width(92.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut mode,
                                            midi_input::Mode::Toggle,
                                            "Toggle",
                                        );
                                        ui.selectable_value(
                                            &mut mode,
                                            midi_input::Mode::Follow,
                                            "Follow",
                                        );
                                    });
                            });
                            if let Some(binding) = binding {
                                if mode != binding.mode {
                                    config.set_binding(
                                        effect,
                                        midi_input::Binding {
                                            trigger: binding.trigger,
                                            mode,
                                        },
                                    );
                                }
                            }

                            ui.horizontal(|ui| {
                                let learning = learn == Some(effect);
                                if ui
                                    .selectable_label(
                                        learning,
                                        if learning { "Move it…" } else { "Learn" },
                                    )
                                    .clicked()
                                {
                                    learn = if learning { None } else { Some(effect) };
                                }
                                if binding.is_some()
                                    && ui
                                        .small_button("×")
                                        .on_hover_text("Forget this control")
                                        .clicked()
                                {
                                    clear = Some(effect);
                                }
                            });
                            ui.end_row();
                        }
                    });

                if let Some(effect) = learn {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "Listening for the {} control — move it on the controller now.",
                            effect.label()
                        ))
                        .font(regular(11.5))
                        .color(AMBER),
                    );
                }

                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "Toggle flips the effect on each press. Follow switches it on while the \
                         control is on, which also suits a latching footswitch that sends 127/0.",
                    )
                    .font(regular(11.0))
                    .color(TEXT_3),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(
                        "A MIDI change edits the live buffer. Recalling a preset on the amp \
                         restores that preset's stored effect states.",
                    )
                    .font(regular(11.0))
                    .color(TEXT_3),
                );
            });

        if !open {
            learn = None;
        }
        if let Some(effect) = clear {
            config.clear_binding(effect);
            if learn == Some(effect) {
                learn = None;
            }
        }
        let old_port = self.midi.port.clone();
        let old_enabled = self.midi.enabled;
        if config != self.midi {
            self.midi = config;
            self.save_midi();
            // Bindings and modes are read by the router on the next event;
            // only a different port or the enable switch needs a reconnect.
            if self.midi.port != old_port || self.midi.enabled != old_enabled {
                reconnect = true;
            }
        }
        self.midi_learn = learn;
        self.show_midi = open;
        if refresh {
            self.refresh_midi_ports();
        }
        if reconnect {
            self.start_midi();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture;

    fn demo_app() -> (egui::Context, App) {
        let ctx = egui::Context::default();
        super::super::theme::install(&ctx);
        let mut app = App::fresh(ctx.clone());
        let reports = capture::amp_reports_from_bytes(super::super::DEMO_CAPTURE)
            .expect("the demo capture parses");
        app.dump = protocol::Dump::from_reports(reports);
        app.original = app.dump.clone();
        app.synced = true;
        (ctx, app)
    }

    #[test]
    fn the_dialog_renders_without_an_input() {
        let (ctx, mut app) = demo_app();
        app.show_midi = true;
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.midi_window(ctx));
        }
        assert!(app.show_midi);
    }

    #[test]
    fn the_dialog_renders_a_learned_control() {
        use crate::midi::{Binding, Mode, Trigger};

        let (ctx, mut app) = demo_app();
        app.show_midi = true;
        app.midi.port = Some("Footswitch".into());
        app.midi.set_binding(
            midi_input::Effect::Reverb,
            Binding {
                trigger: Trigger::Cc { number: 23 },
                mode: Mode::Follow,
            },
        );
        app.midi_ports = vec!["Footswitch".into()];
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| app.midi_window(ctx));
        }
        assert!(app.show_midi);
    }

    #[test]
    fn a_midi_press_becomes_the_same_bypass_command_as_the_pill() {
        use super::super::worker::Cmd;
        use crate::midi::{Binding, Event, Mode, Trigger};
        use crate::protocol::Kind;

        let (_, mut app) = demo_app();
        app.connected = true;
        app.midi.set_binding(
            midi_input::Effect::Reverb,
            Binding {
                trigger: Trigger::Cc { number: 23 },
                mode: Mode::Toggle,
            },
        );
        app.midi_router.set_config(app.midi.clone());

        let (midi_tx, midi_rx) = std::sync::mpsc::channel();
        app.midi_rx = Some(midi_rx);
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        app.tx = Some(cmd_tx);

        let before = app.effect_states()[&midi_input::Effect::Reverb];
        midi_tx
            .send(Event::Control {
                channel: 1,
                cc: 23,
                value: 127,
            })
            .expect("send the MIDI event");
        app.poll_midi();
        app.send_live_edits();

        let expected = !before;
        assert_eq!(app.effect_states()[&midi_input::Effect::Reverb], expected);
        match cmd_rx.try_recv().expect("a bypass command") {
            Cmd::Bypass { kind, bypassed, .. } => {
                assert_eq!(kind, Kind::Reverb);
                assert_eq!(bypassed, expected);
            }
            _ => panic!("expected a bypass command, not another edit"),
        }
    }
}
