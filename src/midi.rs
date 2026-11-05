//! MIDI foot-controller support.
//!
//! The Super Champ X2 has no MIDI input: its USB interface is a vendor HID
//! device. A MIDI controller therefore drives the amp through the host that is
//! already connected to it. This module turns a controller message into the
//! same fast bypass frame the editor's ON/OFF pill sends.
//!
//! The listener is deliberately dumb. It parses the channel-voice messages we
//! care about and hands them to the caller; mapping, learning and the current
//! effect state stay in one place, whether that is the editor or the headless
//! bridge in `scx2 midi`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::protocol::Kind;

/// The four effect blocks FUSE can bypass with the fast frame.
pub const EFFECTS: [Effect; 4] = [Effect::Stomp, Effect::Mod, Effect::Delay, Effect::Reverb];

/// One of the amp's effect blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Stomp,
    Mod,
    Delay,
    Reverb,
}

impl Effect {
    pub fn kind(self) -> Kind {
        match self {
            Effect::Stomp => Kind::Stomp,
            Effect::Mod => Kind::Mod,
            Effect::Delay => Kind::Delay,
            Effect::Reverb => Kind::Reverb,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Effect::Stomp => "Stomp",
            Effect::Mod => "Mod",
            Effect::Delay => "Delay",
            Effect::Reverb => "Reverb",
        }
    }
}

/// How a controller message maps to the effect's on/off state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Every press flips the effect, whatever its current state is.
    #[default]
    Toggle,
    /// The effect follows the control: on while pressed/high, off otherwise.
    ///
    /// A latching footswitch that sends 127 and 0 works as an absolute on/off,
    /// and a momentary switch works while it is held.
    Follow,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Toggle => "Toggle",
            Mode::Follow => "Follow",
        }
    }
}

/// What a learned control is: a CC number, a note number or a program number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Trigger {
    Cc { number: u8 },
    Note { number: u8 },
    Pc { number: u8 },
}

impl Trigger {
    pub fn label(self) -> String {
        match self {
            Trigger::Cc { number } => format!("CC {number}"),
            Trigger::Note { number } => format!("Note {number}"),
            Trigger::Pc { number } => format!("PC {number}"),
        }
    }
}

/// One learned control and the way it should drive its effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub trigger: Trigger,
    #[serde(default)]
    pub mode: Mode,
}

/// The part of the MIDI settings that is worth keeping between runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether the editor should keep the listener open.
    pub enabled: bool,
    /// MIDI input port name. `None` means no controller selected.
    pub port: Option<String>,
    /// Only listen to this channel (1..=16); `None` listens to every channel.
    pub channel: Option<u8>,
    /// The learned control for each effect. Unmapped effects are absent.
    pub bindings: BTreeMap<Effect, Binding>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            port: None,
            channel: None,
            bindings: BTreeMap::new(),
        }
    }
}

impl Config {
    /// The editor's settings file. `SCX2_CONFIG_DIR` overrides the platform
    /// directory, which also keeps tests away from the real one.
    pub fn path() -> Option<PathBuf> {
        config_dir().map(|dir| dir.join("midi.json"))
    }

    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), None);
        };
        match Self::load_from(&path) {
            Ok(config) => (config, None),
            Err(error) => (
                Self::default(),
                Some(format!("Could not read {}: {error:#}", path.display())),
            ),
        }
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("no configuration directory for this platform")?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    pub fn binding(&self, effect: Effect) -> Option<Binding> {
        self.bindings.get(&effect).copied()
    }

    /// Learn or change one control. A trigger can only belong to one effect,
    /// so the old owner is cleared rather than left ambiguous.
    pub fn set_binding(&mut self, effect: Effect, binding: Binding) {
        self.bindings
            .retain(|other, existing| *other == effect || existing.trigger != binding.trigger);
        self.bindings.insert(effect, binding);
    }

    pub fn clear_binding(&mut self, effect: Effect) {
        self.bindings.remove(&effect);
    }
}

fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SCX2_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("scx2"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|dir| {
            PathBuf::from(dir)
                .join("Library")
                .join("Application Support")
                .join("scx2")
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|dir| PathBuf::from(dir).join(".config")))
            .map(|dir| dir.join("scx2"))
    }
}

/// A channel-voice message we know how to map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Control { channel: u8, cc: u8, value: u8 },
    Note { channel: u8, note: u8, on: bool },
    Program { channel: u8, program: u8 },
}

impl Event {
    pub fn channel(self) -> u8 {
        match self {
            Event::Control { channel, .. }
            | Event::Note { channel, .. }
            | Event::Program { channel, .. } => channel,
        }
    }

    pub fn trigger(self) -> Trigger {
        match self {
            Event::Control { cc, .. } => Trigger::Cc { number: cc },
            Event::Note { note, .. } => Trigger::Note { number: note },
            Event::Program { program, .. } => Trigger::Pc { number: program },
        }
    }

    /// A switch turning on: Note On, CC at or above the halfway point, or any
    /// Program Change (which has no release of its own).
    pub fn is_press(self) -> bool {
        match self {
            Event::Control { value, .. } => value >= 64,
            Event::Note { on, .. } => on,
            Event::Program { .. } => true,
        }
    }

    /// A switch turning off: Note Off or a CC below the halfway point.
    pub fn is_release(self) -> bool {
        match self {
            Event::Control { value, .. } => value < 64,
            Event::Note { on, .. } => !on,
            Event::Program { .. } => false,
        }
    }
}

/// Parse one MIDI message. Clock, active sensing, pitch bend and other
/// messages that cannot drive a switch are ignored.
pub fn parse(message: &[u8]) -> Option<Event> {
    let status = *message.first()?;
    let channel = (status & 0x0F) + 1;
    match status & 0xF0 {
        0x80 if message.len() >= 3 => Some(Event::Note {
            channel,
            note: message[1],
            on: false,
        }),
        0x90 if message.len() >= 3 => Some(Event::Note {
            channel,
            note: message[1],
            on: message[2] > 0,
        }),
        0xB0 if message.len() >= 3 => Some(Event::Control {
            channel,
            cc: message[1],
            value: message[2],
        }),
        0xC0 if message.len() >= 2 => Some(Event::Program {
            channel,
            program: message[1],
        }),
        _ => None,
    }
}

/// A connected MIDI input. Dropping it closes the port.
pub struct Listener {
    /// Kept alive for the lifetime of the listener; dropping it closes the port.
    _connection: midir::MidiInputConnection<()>,
    port: String,
}

impl Listener {
    pub fn port(&self) -> &str {
        &self.port
    }
}

/// List the MIDI inputs the system exposes, by name.
pub fn ports() -> Result<Vec<String>> {
    let input = midir::MidiInput::new("scx2")?;
    let mut names = Vec::new();
    for port in input.ports() {
        match input.port_name(&port) {
            Ok(name) => names.push(name),
            Err(_) => names.push("Unknown MIDI input".into()),
        }
    }
    Ok(names)
}

/// Open `port_name` and call `callback` for every message we can map.
///
/// The callback runs on midir's own thread, so it must not block. The editor
/// uses it to send into a channel and wake the UI.
pub fn connect<F>(port_name: &str, mut callback: F) -> Result<Listener>
where
    F: FnMut(Event) + Send + 'static,
{
    let input = midir::MidiInput::new("scx2")?;
    let port = input
        .ports()
        .into_iter()
        .find(|port| input.port_name(port).is_ok_and(|name| name == port_name))
        .with_context(|| format!("MIDI input {port_name:?} is not connected"))?;
    let connection = input
        .connect(
            &port,
            "scx2-midi",
            move |_stamp, message, _| {
                if let Some(event) = parse(message) {
                    callback(event);
                }
            },
            (),
        )
        .map_err(|error| anyhow::anyhow!("opening MIDI input {port_name:?}: {error}"))?;
    Ok(Listener {
        _connection: connection,
        port: port_name.to_string(),
    })
}

/// A bypass write the bridge should make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub effect: Effect,
    /// `true` bypasses the effect (off), `false` enables it (on).
    pub bypassed: bool,
}

/// Turns MIDI events into bypass actions using the learned bindings.
///
/// It is stateless: `Toggle` asks the caller for the effect's current state,
/// and `Follow` compares the requested state with the current one. That keeps
/// the bridge in step with front-panel changes and preset recalls.
#[derive(Clone, Debug)]
pub struct Router {
    config: Config,
}

impl Router {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn handle(&self, event: Event, current: &BTreeMap<Effect, bool>) -> Option<Action> {
        if self
            .config
            .channel
            .is_some_and(|channel| channel != event.channel())
        {
            return None;
        }
        let trigger = event.trigger();
        let (effect, binding) = self
            .config
            .bindings
            .iter()
            .find(|(_, binding)| binding.trigger == trigger)?;
        let effect = *effect;
        let bypassed = match binding.mode {
            Mode::Toggle => {
                if !event.is_press() {
                    return None;
                }
                !current.get(&effect).copied().unwrap_or(false)
            }
            Mode::Follow => {
                if !event.is_press() && !event.is_release() {
                    return None;
                }
                // A press means "enable", a release means "bypass".
                !event.is_press()
            }
        };
        if current.get(&effect) == Some(&bypassed) {
            return None;
        }
        Some(Action { effect, bypassed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound(effect: Effect, trigger: Trigger, mode: Mode) -> Config {
        let mut config = Config::default();
        config.set_binding(effect, Binding { trigger, mode });
        config
    }

    fn states(pairs: &[(Effect, bool)]) -> BTreeMap<Effect, bool> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn parses_the_channel_voice_messages_a_pedal_sends() {
        assert_eq!(
            parse(&[0xB3, 20, 127]),
            Some(Event::Control {
                channel: 4,
                cc: 20,
                value: 127
            })
        );
        assert_eq!(
            parse(&[0x90, 60, 100]),
            Some(Event::Note {
                channel: 1,
                note: 60,
                on: true
            })
        );
        assert_eq!(
            parse(&[0x80, 60, 0]),
            Some(Event::Note {
                channel: 1,
                note: 60,
                on: false
            })
        );
        assert_eq!(
            parse(&[0xC2, 7]),
            Some(Event::Program {
                channel: 3,
                program: 7
            })
        );
        assert_eq!(parse(&[0xF8]), None, "clock is not a switch");
        assert_eq!(parse(&[0xE0, 0, 64]), None, "pitch bend is ignored");
    }

    #[test]
    fn a_note_on_with_velocity_zero_is_a_release() {
        assert!(parse(&[0x90, 60, 0]).is_some_and(|event| event.is_release()));
    }

    #[test]
    fn config_round_trips_through_json() {
        let mut config = bound(Effect::Reverb, Trigger::Cc { number: 23 }, Mode::Follow);
        config.port = Some("Footswitch".into());
        config.channel = Some(2);
        config.set_binding(
            Effect::Stomp,
            Binding {
                trigger: Trigger::Note { number: 36 },
                mode: Mode::Toggle,
            },
        );

        let json = serde_json::to_string(&config).expect("serialise");
        assert!(json.contains("\"reverb\""), "{json}");
        let back: Config = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, config);
    }

    #[test]
    fn config_saves_and_loads_from_a_file() {
        let mut path = std::env::temp_dir();
        path.push(format!("scx2-midi-test-{}.json", std::process::id()));
        let config = bound(Effect::Delay, Trigger::Note { number: 64 }, Mode::Toggle);

        config.save_to(&path).expect("save the settings");
        let back = Config::load_from(&path).expect("load the settings");
        assert_eq!(back, config);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn learning_one_trigger_clears_the_other_owner() {
        let mut config = bound(Effect::Stomp, Trigger::Cc { number: 20 }, Mode::Toggle);
        config.set_binding(
            Effect::Reverb,
            Binding {
                trigger: Trigger::Cc { number: 20 },
                mode: Mode::Toggle,
            },
        );
        assert!(config.binding(Effect::Stomp).is_none());
        assert_eq!(
            config
                .binding(Effect::Reverb)
                .map(|binding| binding.trigger),
            Some(Trigger::Cc { number: 20 })
        );
    }

    #[test]
    fn toggle_flips_the_effect_it_is_bound_to() {
        let router = Router::new(bound(
            Effect::Delay,
            Trigger::Cc { number: 21 },
            Mode::Toggle,
        ));
        let on = states(&[(Effect::Delay, false)]);
        let action = router
            .handle(
                Event::Control {
                    channel: 1,
                    cc: 21,
                    value: 127,
                },
                &on,
            )
            .expect("the press is bound");
        assert_eq!(
            action,
            Action {
                effect: Effect::Delay,
                bypassed: true
            }
        );
        assert!(
            router
                .handle(
                    Event::Control {
                        channel: 1,
                        cc: 21,
                        value: 0
                    },
                    &on
                )
                .is_none(),
            "the release must not toggle it back"
        );
    }

    #[test]
    fn follow_tracks_the_control_value() {
        let router = Router::new(bound(
            Effect::Reverb,
            Trigger::Cc { number: 22 },
            Mode::Follow,
        ));
        let on = states(&[(Effect::Reverb, false)]);
        assert!(
            router
                .handle(
                    Event::Control {
                        channel: 1,
                        cc: 22,
                        value: 127
                    },
                    &on
                )
                .is_none(),
            "already on, so nothing to send"
        );
        let action = router
            .handle(
                Event::Control {
                    channel: 1,
                    cc: 22,
                    value: 0,
                },
                &on,
            )
            .expect("releasing turns it off");
        assert_eq!(
            action,
            Action {
                effect: Effect::Reverb,
                bypassed: true
            }
        );
    }

    #[test]
    fn program_change_works_for_pedals_without_cc() {
        let router = Router::new(bound(
            Effect::Stomp,
            Trigger::Pc { number: 5 },
            Mode::Toggle,
        ));
        let state = states(&[(Effect::Stomp, false)]);
        assert_eq!(
            router.handle(
                Event::Program {
                    channel: 1,
                    program: 5
                },
                &state
            ),
            Some(Action {
                effect: Effect::Stomp,
                bypassed: true
            })
        );
    }

    #[test]
    fn a_channel_filter_ignores_other_channels() {
        let mut config = bound(Effect::Mod, Trigger::Cc { number: 30 }, Mode::Toggle);
        config.channel = Some(2);
        let router = Router::new(config);
        let state = states(&[(Effect::Mod, false)]);
        assert!(router
            .handle(
                Event::Control {
                    channel: 1,
                    cc: 30,
                    value: 127
                },
                &state
            )
            .is_none());
        assert!(router
            .handle(
                Event::Control {
                    channel: 2,
                    cc: 30,
                    value: 127
                },
                &state
            )
            .is_some());
    }

    #[test]
    fn unbound_controls_are_ignored() {
        let router = Router::new(bound(Effect::Mod, Trigger::Cc { number: 30 }, Mode::Toggle));
        assert!(router
            .handle(
                Event::Control {
                    channel: 1,
                    cc: 31,
                    value: 127
                },
                &states(&[(Effect::Mod, false)])
            )
            .is_none());
    }
}
