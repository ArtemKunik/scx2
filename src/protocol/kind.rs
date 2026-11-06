/// Block type, byte 2 of a state report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    State,
    Name,
    Amp,
    Stomp,
    Mod,
    Delay,
    Reverb,
    Pedal,
    Misc,
    Unknown(u8),
}

impl Kind {
    pub fn from_code(code: u8) -> Self {
        match code {
            0x00 => Kind::State,
            0x04 => Kind::Name,
            0x05 => Kind::Amp,
            0x06 => Kind::Stomp,
            0x07 => Kind::Mod,
            0x08 => Kind::Delay,
            0x09 => Kind::Reverb,
            0x0A => Kind::Pedal,
            0x0D => Kind::Misc,
            other => Kind::Unknown(other),
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Kind::State => 0x00,
            Kind::Name => 0x04,
            Kind::Amp => 0x05,
            Kind::Stomp => 0x06,
            Kind::Mod => 0x07,
            Kind::Delay => 0x08,
            Kind::Reverb => 0x09,
            Kind::Pedal => 0x0A,
            Kind::Misc => 0x0D,
            Kind::Unknown(code) => code,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::State => "state",
            Kind::Name => "name",
            Kind::Amp => "amp",
            Kind::Stomp => "stomp",
            Kind::Mod => "mod",
            Kind::Delay => "delay",
            Kind::Reverb => "reverb",
            Kind::Pedal => "pedal",
            Kind::Misc => "misc",
            Kind::Unknown(_) => "unknown",
        }
    }

    /// Whether byte 16 carries a DSP model id for this block.
    pub fn has_model(self) -> bool {
        matches!(
            self,
            Kind::Amp | Kind::Stomp | Kind::Mod | Kind::Delay | Kind::Reverb | Kind::Pedal
        )
    }

    /// Whether byte 18 carries the effect slot.
    pub fn has_slot(self) -> bool {
        matches!(
            self,
            Kind::Stomp | Kind::Mod | Kind::Delay | Kind::Reverb | Kind::Pedal
        )
    }

    /// Whether byte 3/4 are meaningful preset coordinates for this block.
    ///
    /// Pedal and misc reports reuse those bytes for other things, so they are
    /// kept out of the preset map.
    pub fn is_preset_scoped(self) -> bool {
        matches!(
            self,
            Kind::State
                | Kind::Name
                | Kind::Amp
                | Kind::Stomp
                | Kind::Mod
                | Kind::Delay
                | Kind::Reverb
        )
    }

    /// Parse a block name as used on the command line.
    pub fn parse(text: &str) -> Option<Kind> {
        Some(match text.to_ascii_lowercase().as_str() {
            "state" => Kind::State,
            "name" => Kind::Name,
            "amp" => Kind::Amp,
            "stomp" => Kind::Stomp,
            "mod" => Kind::Mod,
            "delay" => Kind::Delay,
            "reverb" => Kind::Reverb,
            "pedal" => Kind::Pedal,
            "misc" => Kind::Misc,
            _ => return None,
        })
    }
}

/// The sonic blocks a preset bank stores.
///
/// Bank 1 (amp presets) keeps the amp and stomp blocks; bank 2 (FX presets)
/// keeps modulation, delay and reverb. The `state`/`misc` reports in a dump are
/// derived, and a live buffer carries every block regardless of the target
/// bank, so a store read-back must compare only these.
pub fn bank_kinds(bank: u8) -> &'static [Kind] {
    match bank {
        super::BANK_AMP => &[Kind::Amp, Kind::Stomp],
        super::BANK_FX => &[Kind::Mod, Kind::Delay, Kind::Reverb],
        _ => &[],
    }
}
