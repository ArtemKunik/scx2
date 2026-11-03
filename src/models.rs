//! DSP model id -> name tables.
//!
//! The Super Champ X2 speaks the same model id space as the Mustang family.
//! The ids below were cross-checked against a real SCX2 dump: every model the
//! amp reported (amp, stomp, mod, delay, reverb) resolved to the expected
//! preset name, see `tests/protocol.rs`.
//!
//! The 0xea / 0xeb amp ids are SCX2-only voices and do not exist on Mustang.

use crate::protocol::Kind;

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

/// Look up a model name for a block kind.
pub fn model_name(kind: Kind, id: u16) -> Option<&'static str> {
    let table = match kind {
        Kind::Amp => AMP_MODELS,
        Kind::Stomp => STOMP_MODELS,
        Kind::Mod => MOD_MODELS,
        Kind::Delay => DELAY_MODELS,
        Kind::Reverb => REVERB_MODELS,
        _ => return None,
    };
    table
        .iter()
        .find(|(model, _)| *model == id)
        .map(|(_, name)| *name)
}

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
