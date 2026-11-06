//! DSP model id -> name tables.
//!
//! The Super Champ X2 speaks the same model id space as the Mustang family.
//! The ids below were cross-checked against a real SCX2 dump: every model the
//! amp reported (amp, stomp, mod, delay, reverb) resolved to the expected
//! preset name, see `tests/protocol.rs`.
//!
//! The 0xea / 0xeb amp ids are SCX2-only voices and do not exist on Mustang.

use crate::protocol::Kind;

/// The amp model the SCX2 reports on channel 1, the clean tube channel.
///
/// A `.fuse`-era dump has it in amp slot 16 (`Clean Channel`), which is where
/// SAVE CH1 writes.
pub const CLEAN_CHANNEL_MODEL: u16 = 0x00eb;

pub const AMP_MODELS: &[(u16, &str)] = &[
    (0x0000, "None"),
    (0x0067, "'57 Deluxe"),
    (0x0064, "'59 Bassman"),
    (0x007c, "'57 Champ"),
    (0x0053, "'65 Deluxe"),
    (0x006a, "'65 Princeton"),
    (0x0075, "'65 Twin"),
    (0x0072, "Super-Sonic"),
    (0x0061, "British '60s"),
    (0x0079, "British '70s"),
    (0x005e, "British '80s"),
    (0x005d, "American '90s"),
    (0x006d, "Metal 2000"),
    (0x00f1, "Studio Preamp"),
    (0x00f6, "'57 Twin"),
    (0x00f9, "'60s Thrift"),
    (0x00ff, "British Watts"),
    (0x00fc, "British Colour"),
    // Super Champ X2 only
    (0x00ea, "JazzMaster"),
    (0x00eb, "Clean Channel"),
];

/// Cabinets FUSE offers for the amp block, in the order the amp stores them
/// at offset 49.
///
/// The ids and identities are FUSE's own cabinet enum
/// (`eFXPARM_CABSIM_*` from FUSE 2.7.1). `0` is direct / no simulation, and
/// the SCX2 firmware gives every voice one of these as its default cabinet.
pub const CABINET_MODELS: &[(u8, &str)] = &[
    (0, "Off (direct)"),
    (1, "1x12 '57 Deluxe"),
    (2, "4x10 '59 Bassman"),
    (3, "1x12 '65 Deluxe"),
    (4, "1x10 '65 Princeton"),
    (5, "1x8 '57 Champ"),
    (6, "4x12 JCM800"),
    (7, "2x12 AC30"),
    (8, "4x12 EVH"),
    (9, "2x12 '65 Twin"),
    (10, "4x12 Recto"),
    (11, "2x12 Super-Sonic"),
    (12, "1x12 Super-Sonic"),
];

/// Highest cabinet id FUSE writes for the amp block.
pub const CABINET_MAX: u8 = 0x0c;

/// The cabinet model stored at block offset 49, by FUSE's id.
pub fn cabinet_name(id: u8) -> Option<&'static str> {
    CABINET_MODELS
        .iter()
        .find(|(cabinet, _)| *cabinet == id)
        .map(|(_, name)| *name)
}

pub const STOMP_MODELS: &[(u16, &str)] = &[
    (0x0000, "None"),
    (0x003c, "Overdrive"),
    (0x0049, "Wah"),
    (0x004a, "Touch Wah"),
    (0x001a, "Fuzz"),
    (0x001c, "Fuzz Touch Wah"),
    (0x0088, "Simple Compressor"),
    (0x0007, "Compressor"),
    (0x0103, "Ranger Boost"),
    (0x00ba, "Green Box"),
    (0x0110, "Orange Box"),
    (0x0111, "Black Box"),
    (0x010f, "Big Fuzz"),
];

pub const MOD_MODELS: &[(u16, &str)] = &[
    (0x0000, "None"),
    (0x0012, "Sine Chorus"),
    (0x0013, "Triangle Chorus"),
    (0x0018, "Sine Flanger"),
    (0x0019, "Triangle Flanger"),
    (0x002d, "Vibratone"),
    (0x0040, "Vintage Tremolo"),
    (0x0041, "Sine Tremolo"),
    (0x0022, "Ring Modulator"),
    (0x0029, "Step Filter"),
    (0x004f, "Phaser"),
    (0x001f, "Pitch Shifter"),
    (0x00f4, "Mod Wah"),
    (0x00f5, "Mod Touch Wah"),
    (0x101f, "Diatonic Pitch Shift"),
];

pub const DELAY_MODELS: &[(u16, &str)] = &[
    (0x0000, "None"),
    (0x0016, "Mono Delay"),
    (0x0043, "Mono Echo Filter"),
    (0x0048, "Stereo Echo Filter"),
    (0x0044, "Multitap Delay"),
    (0x0045, "Ping Pong Delay"),
    (0x0015, "Ducking Delay"),
    (0x0046, "Reverse Delay"),
    (0x002b, "Tape Delay"),
    (0x002a, "Stereo Tape Delay"),
];

pub const REVERB_MODELS: &[(u16, &str)] = &[
    (0x0000, "None"),
    (0x0024, "Small Hall"),
    (0x003a, "Large Hall"),
    (0x0026, "Small Room"),
    (0x003b, "Large Room"),
    (0x004e, "Small Plate"),
    (0x004b, "Large Plate"),
    (0x004c, "Ambient"),
    (0x004d, "Arena"),
    (0x0021, "Spring '63"),
    (0x000b, "Spring '65"),
];

/// The model table a block kind chooses from, `None` for blocks without one.
///
/// The first entry is always `(0, "None")`, which is how an empty effect slot
/// is represented on the wire.
pub fn table(kind: Kind) -> Option<&'static [(u16, &'static str)]> {
    Some(match kind {
        Kind::Amp => AMP_MODELS,
        Kind::Stomp => STOMP_MODELS,
        Kind::Mod => MOD_MODELS,
        Kind::Delay => DELAY_MODELS,
        Kind::Reverb => REVERB_MODELS,
        _ => return None,
    })
}

/// Look up a model name for a block kind.
pub fn model_name(kind: Kind, id: u16) -> Option<&'static str> {
    table(kind)?
        .iter()
        .find(|(model, _)| *model == id)
        .map(|(_, name)| *name)
}

// ---------------------------------------------------------------------------
// effect parameter names
// ---------------------------------------------------------------------------
//
// FUSE describes every DSP model with a fixed six-slot parameter list. The
// names below come from FUSE's own model definitions (the same table the
// `refuse` project publishes); they are not yet verified against SCX2
// hardware, but they are the real control names rather than artwork.
//
// An empty string means the model does not use that slot. The number of
// leading non-empty entries is the model's knob count.

const LEVEL_TIME_FDBK_BRIGHT_ATTEN: &[&str] = &["Level", "Time", "Fdbk", "Bright", "Atten", ""];
const LEVEL_RATE_DEPTH_AVG_PHASE: &[&str] = &["Level", "Rate", "Depth", "Avg Dly", "LR Phase", ""];
const LEVEL_RATE_DEPTH_FDBK_PHASE: &[&str] = &["Level", "Rate", "Depth", "Fdbk", "LR Phase", ""];
const REVERB: &[&str] = &["Level", "Decay", "Dwell", "Diff", "Tone", ""];
const DRIVE: &[&str] = &["Level", "Gain", "Low", "Mid", "High", ""];
const WAH: &[&str] = &["Level", "Freq", "Min", "Max", "Q", ""];
const TOUCH_WAH: &[&str] = &["Level", "Sens", "Min", "Max", "Q", ""];
const COMPRESSOR: &[&str] = &["Level", "Thresh", "Ratio", "Attack", "Release", ""];
const PITCH: &[&str] = &["Level", "Pitch", "Detune", "Fdbk", "PreDly", ""];

/// Parameter labels for an effect model, in block-offset order from 32.
///
/// The first entry is offset 32. Unused slots are empty strings, so callers can
/// count the non-empty prefix to know how many knobs the model really has.
pub fn effect_params(kind: Kind, model: u16) -> &'static [&'static str] {
    match (kind, model) {
        (Kind::Stomp, 0x003c) => DRIVE,
        (Kind::Stomp, 0x0049) => WAH,
        (Kind::Stomp, 0x004a) => TOUCH_WAH,
        (Kind::Stomp, 0x001a) => &["Level", "Gain", "Octave", "Low", "High", ""],
        (Kind::Stomp, 0x001c) => &["Level", "Gain", "Sens", "Octave", "Peak", ""],
        (Kind::Stomp, 0x0088) => &["Type", "", "", "", "", ""],
        (Kind::Stomp, 0x0007) => COMPRESSOR,
        (Kind::Stomp, 0x0103) => &["Level", "Gain", "Tone", "", "", ""],
        (Kind::Stomp, 0x00ba) => &["Level", "Gain", "Tone", "Blend", "", ""],
        (Kind::Stomp, 0x0110) => &["Level", "Gain", "Tone", "", "", ""],
        (Kind::Stomp, 0x0111) => &["Level", "Gain", "Tone", "", "", ""],
        (Kind::Stomp, 0x010f) => &["Level", "Tone", "Sustain", "", "", ""],

        (Kind::Mod, 0x0012 | 0x0013) => LEVEL_RATE_DEPTH_AVG_PHASE,
        (Kind::Mod, 0x0018 | 0x0019) => LEVEL_RATE_DEPTH_FDBK_PHASE,
        (Kind::Mod, 0x002d) => &["Level", "Rotor", "Depth", "Fdbk", "LR Phase", ""],
        (Kind::Mod, 0x0040) => &["Level", "Rate", "Duty", "Attack", "Release", ""],
        (Kind::Mod, 0x0041) => &["Level", "Rate", "Duty", "LFO Clip", "Tri Shape", ""],
        (Kind::Mod, 0x0022) => &["Level", "Freq", "Depth", "Shape", "Phase", ""],
        (Kind::Mod, 0x0029) => &["Level", "Rate", "Res", "Min Freq", "Max Freq", ""],
        (Kind::Mod, 0x004f) => &["Level", "Rate", "Depth", "Fdbk", "Shape", ""],
        (Kind::Mod, 0x001f | 0x101f) => PITCH,
        (Kind::Mod, 0x00f4) => WAH,
        (Kind::Mod, 0x00f5) => TOUCH_WAH,

        (Kind::Delay, 0x0016) => LEVEL_TIME_FDBK_BRIGHT_ATTEN,
        (Kind::Delay, 0x0043 | 0x0048) => &["Level", "Time", "Fdbk", "Freq", "Res", "Input"],
        (Kind::Delay, 0x0044..=0x0046) => LEVEL_TIME_FDBK_BRIGHT_ATTEN,
        (Kind::Delay, 0x0015) => &["Level", "Time", "Fdbk", "Release", "Thresh", ""],
        (Kind::Delay, 0x002b) => &["Level", "Time", "Fdbk", "Flutter", "Bright", "Stereo"],
        (Kind::Delay, 0x002a) => &["Level", "Time", "Fdbk", "Flutter", "Sep", "Bright"],

        (Kind::Reverb, _) => REVERB,
        _ => &[],
    }
}

/// How many of an effect model's parameter slots are used.
pub fn effect_knobs(kind: Kind, model: u16) -> usize {
    effect_params(kind, model)
        .iter()
        .take_while(|label| !label.is_empty())
        .count()
}

// ---------------------------------------------------------------------------
// factory defaults
// ---------------------------------------------------------------------------
//
// Known six-byte parameter defaults, captured by the FUSE/refuse work. Models
// without an entry get a neutral 0x80 in every used slot, so loading a model
// never inherits the previous effect's settings.

const DEFAULTS: &[(Kind, u16, &[u8])] = &[
    (Kind::Stomp, 0x003c, &[0x80, 0x80, 0x80, 0x80, 0x80]),
    (Kind::Stomp, 0x0049, &[0xff, 0x80, 0x00, 0xff, 0x00]),
    (Kind::Stomp, 0x001a, &[0x80, 0x80, 0x80, 0x80, 0x80]),
    (Kind::Stomp, 0x0088, &[0x01]),
    (Kind::Stomp, 0x0007, &[0x8d, 0x0f, 0x4f, 0x7f, 0x7f]),
    (Kind::Stomp, 0x0103, &[0x64, 0xba, 0x01, 0x9b]),
    (Kind::Stomp, 0x00ba, &[0x81, 0xb1, 0x8c, 0xff]),
    (Kind::Stomp, 0x0110, &[0x81, 0x81, 0x81]),
    (Kind::Stomp, 0x0111, &[0x81, 0x81, 0x56]),
    (Kind::Stomp, 0x010f, &[0xac, 0xac, 0x73]),
    (Kind::Mod, 0x0012, &[0xff, 0x0e, 0x19, 0x19, 0x80]),
    (Kind::Mod, 0x0018, &[0xff, 0x0e, 0x80, 0x80, 0x80]),
    (Kind::Mod, 0x0040, &[0xdb, 0xad, 0x63, 0xf4, 0xf1]),
    (Kind::Mod, 0x004f, &[0xfd, 0x00, 0xfd, 0xb8, 0x00]),
    (Kind::Delay, 0x0016, &[0xff, 0x80, 0x80, 0x80, 0x80]),
    (Kind::Delay, 0x002b, &[0x7d, 0x1c, 0x00, 0x63, 0x80]),
    (Kind::Delay, 0x002a, &[0x7d, 0x88, 0x1c, 0x63, 0xff, 0x80]),
    (Kind::Reverb, 0x0024, &[0x6e, 0x5d, 0x6e, 0x80, 0x91]),
    (Kind::Reverb, 0x003a, &[0x4f, 0x3e, 0x80, 0x05, 0xb0]),
    (Kind::Reverb, 0x0026, &[0x80, 0x80, 0x80, 0x80, 0x80]),
    (Kind::Reverb, 0x004b, &[0x38, 0x80, 0x91, 0x80, 0xb6]),
    (Kind::Reverb, 0x000b, &[0x80, 0x8b, 0x49, 0xff, 0x80]),
];

/// Default parameter bytes (block offsets 32..63) for an effect model.
pub fn default_params(kind: Kind, model: u16) -> Option<[u8; 32]> {
    let labels = effect_params(kind, model);
    if labels.is_empty() {
        return None;
    }
    let mut params = [0u8; 32];
    match DEFAULTS
        .iter()
        .find(|(entry_kind, entry_model, _)| *entry_kind == kind && *entry_model == model)
    {
        Some((_, _, values)) => params[..values.len()].copy_from_slice(values),
        None => {
            for (index, label) in labels.iter().enumerate() {
                if !label.is_empty() {
                    params[index] = 0x80;
                }
            }
        }
    }
    Some(params)
}

// ---------------------------------------------------------------------------
// FUSE XML ids
// ---------------------------------------------------------------------------
//
// A `.fuse` preset refers to models by FUSE's own integer ids, which are not
// the DSP model ids. This table is the mapping FUSE uses.

const FUSE_IDS: &[(Kind, u16, u32)] = &[
    (Kind::Amp, 0x0067, 0),
    (Kind::Amp, 0x0064, 1),
    (Kind::Amp, 0x007c, 2),
    (Kind::Amp, 0x0053, 3),
    (Kind::Amp, 0x006a, 4),
    (Kind::Amp, 0x0075, 5),
    (Kind::Amp, 0x0072, 6),
    (Kind::Amp, 0x0061, 7),
    (Kind::Amp, 0x0079, 8),
    (Kind::Amp, 0x005e, 9),
    (Kind::Amp, 0x005d, 10),
    (Kind::Amp, 0x006d, 11),
    (Kind::Amp, 0x00f1, 100),
    (Kind::Amp, 0x00f6, 101),
    (Kind::Amp, 0x00f9, 102),
    (Kind::Amp, 0x00ff, 103),
    (Kind::Amp, 0x00fc, 104),
    (Kind::Stomp, 0x003c, 19),
    (Kind::Stomp, 0x0049, 20),
    (Kind::Stomp, 0x004a, 21),
    (Kind::Stomp, 0x001a, 22),
    (Kind::Stomp, 0x001c, 23),
    (Kind::Stomp, 0x0007, 24),
    (Kind::Stomp, 0x0088, 25),
    (Kind::Stomp, 0x0103, 109),
    (Kind::Stomp, 0x00ba, 110),
    (Kind::Stomp, 0x0110, 111),
    (Kind::Stomp, 0x0111, 112),
    (Kind::Stomp, 0x010f, 113),
    (Kind::Mod, 0x0012, 26),
    (Kind::Mod, 0x0013, 27),
    (Kind::Mod, 0x0018, 28),
    (Kind::Mod, 0x0019, 29),
    (Kind::Mod, 0x002d, 30),
    (Kind::Mod, 0x0040, 31),
    (Kind::Mod, 0x0041, 32),
    (Kind::Mod, 0x0022, 33),
    (Kind::Mod, 0x0029, 34),
    (Kind::Mod, 0x004f, 35),
    (Kind::Mod, 0x001f, 36),
    (Kind::Delay, 0x0016, 37),
    (Kind::Delay, 0x002b, 38),
    (Kind::Delay, 0x0015, 39),
    (Kind::Delay, 0x0046, 40),
    (Kind::Delay, 0x0048, 41),
    (Kind::Reverb, 0x0024, 42),
    (Kind::Reverb, 0x003a, 43),
    (Kind::Reverb, 0x0026, 44),
    (Kind::Reverb, 0x003b, 45),
    (Kind::Reverb, 0x004e, 46),
    (Kind::Reverb, 0x004b, 47),
    (Kind::Reverb, 0x004c, 48),
    (Kind::Reverb, 0x004d, 49),
    (Kind::Reverb, 0x0021, 50),
    (Kind::Reverb, 0x000b, 51),
];

/// The FUSE XML id for a model, if FUSE knows it.
pub fn fuse_id(kind: Kind, model: u16) -> Option<u32> {
    FUSE_IDS
        .iter()
        .find(|(entry_kind, entry_model, _)| *entry_kind == kind && *entry_model == model)
        .map(|(_, _, id)| *id)
}

/// Resolve a FUSE XML model id, including the duplicate ids FUSE used for the
/// V2 re-issues (105..108 for the Deluxe/Princeton/Twin/Super-Sonic).
pub fn from_fuse_id(kind: Kind, id: u32) -> Option<u16> {
    if let Some((_, model, _)) = FUSE_IDS
        .iter()
        .find(|(entry_kind, _, entry_id)| *entry_kind == kind && *entry_id == id)
    {
        return Some(*model);
    }
    match (kind, id) {
        (Kind::Amp, 105) => Some(0x0053),
        (Kind::Amp, 106) => Some(0x006a),
        (Kind::Amp, 107) => Some(0x0075),
        (Kind::Amp, 108) => Some(0x0072),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// what this SCX2 actually reported
// ---------------------------------------------------------------------------

/// Effect models that appear in the bundled SCX2 capture.
///
/// The picker uses this only as an annotation: the Mustang-wide tables are
/// still offered, because a model that FUSE can load may work even if this one
/// capture never used it.
pub const OBSERVED: &[(Kind, u16)] = &[
    (Kind::Stomp, 0x0007),
    (Kind::Mod, 0x0012),
    (Kind::Mod, 0x002d),
    (Kind::Mod, 0x0041),
    (Kind::Delay, 0x0016),
    (Kind::Delay, 0x002b),
    (Kind::Reverb, 0x000b),
    (Kind::Reverb, 0x0024),
    (Kind::Reverb, 0x003a),
    (Kind::Reverb, 0x003b),
];

/// Whether the bundled SCX2 capture contains this effect model.
pub fn observed(kind: Kind, model: u16) -> bool {
    OBSERVED
        .iter()
        .any(|(entry_kind, entry_model)| *entry_kind == kind && *entry_model == model)
}

/// The highest voice index the SCX2 front panel can select.
pub const VOICE_MAX: u8 = 0x0d;

/// Amp model selected by a voice index (block offsets 44..46); 0 when unknown.
pub fn voice_model_id(index: u8) -> u16 {
    match index {
        0x00 => 0x00eb,
        0x01 => 0x0067,
        0x02 => 0x0064,
        0x03 => 0x0053,
        0x04 => 0x006a,
        0x05 => 0x0075,
        0x06 => 0x0072,
        0x07 => 0x0061,
        0x08 => 0x006d,
        0x09 => 0x005e,
        0x0a => 0x005d,
        0x0b => 0x0079,
        0x0c => 0x007c,
        0x0d => 0x00ea,
        _ => 0,
    }
}

/// The voice index (block offsets 44..46) that selects an amp model, if known.
pub fn voice_index(model: u16) -> Option<u8> {
    (0u8..=VOICE_MAX).find(|index| voice_model_id(*index) == model)
}

/// Every voice the SCX2 can select, in front-panel order.
///
/// These are the fourteen indices the firmware defines. The rest of
/// `AMP_MODELS` are Mustang models this amp does not have.
pub fn voices() -> impl Iterator<Item = (u8, u16, &'static str)> {
    (0u8..=VOICE_MAX).map(|index| {
        let model = voice_model_id(index);
        let name = model_name(Kind::Amp, model).unwrap_or("Unknown");
        (index, model, name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_voice_has_a_name_and_an_index() {
        let voices: Vec<_> = voices().collect();
        assert_eq!(voices.len(), VOICE_MAX as usize + 1);
        for (index, model, name) in voices {
            assert_ne!(name, "Unknown", "voice {index} has no model name");
            assert_eq!(voice_index(model), Some(index));
        }
    }

    #[test]
    fn voices_are_in_front_panel_order() {
        assert_eq!(voice_model_id(0), 0x00eb);
        assert_eq!(voice_model_id(0x03), 0x0053);
        assert_eq!(model_name(Kind::Amp, 0x0053), Some("'65 Deluxe"));
        assert_eq!(voice_model_id(VOICE_MAX), 0x00ea);
    }

    #[test]
    fn fuses_cabinet_simulation_is_fully_covered() {
        assert_eq!(CABINET_MODELS.len(), CABINET_MAX as usize + 1);
        assert_eq!(cabinet_name(0), Some("Off (direct)"));
        assert_eq!(cabinet_name(0x05), Some("1x8 '57 Champ"));
        assert_eq!(cabinet_name(CABINET_MAX), Some("1x12 Super-Sonic"));
        assert_eq!(cabinet_name(CABINET_MAX + 1), None);

        for (id, name) in CABINET_MODELS {
            assert_eq!(cabinet_name(*id), Some(*name));
        }
    }
}
