//! Direct parameter control.
//!
//! The Super Champ X2 accepts the same direct-control frame as the Mustang
//! family: a `05 c3` report carrying the model id, a parameter index and a
//! 16-bit "magic" value. The amp stores the new setting in the live block,
//! which is what a dump reports back.
//!
//! Frame layout (verified on hardware, see docs/protocol.md):
//!
//! ```text
//!  0  0x05
//!  1  0xc3
//!  2  0x02
//!  3  model id low
//!  4  model id high
//!  5  parameter index
//!  6  parameter index (repeat)
//!  7  selector, 0x0c for the amp parameters
//!  9  magic value low
//! 10  magic value high
//! ```

use crate::protocol::{self, REPORT_SIZE};

/// MIDI value (0..127) to the 16-bit value the DSP expects.
///
/// Table from the Mustang protocol reference (`docs/reference/magic.cpp`),
/// GPL-3.0, used here as protocol knowledge.
pub const MAGIC_VALUES: [u16; 128] = [
    0x0000, 0x01e1, 0x036a, 0x04f3, 0x067c, 0x085d, 0x0a69, 0x0fdf, 0x100b, 0x129a, 0x1555, 0x178d,
    0x199a, 0x1af7, 0x1c80, 0x1fbe, 0x2016, 0x21cb, 0x2354, 0x2486, 0x263b, 0x2873, 0x29fc, 0x2f9e,
    0x30a4, 0x3333, 0x3514, 0x36f4, 0x3826, 0x39db, 0x3b64, 0x3fd4, 0x402b, 0x41e0, 0x43ed, 0x45f9,
    0x47ae, 0x490b, 0x4ac0, 0x4f88, 0x500b, 0x5217, 0x547b, 0x5604, 0x57e4, 0x5999, 0x5ba6, 0x5fea,
    0x6041, 0x6279, 0x642e, 0x6692, 0x6872, 0x6a53, 0x6c33, 0x6ff5, 0x7020, 0x7201, 0x7439, 0x7619,
    0x7851, 0x79af, 0x7b90, 0x7fff, 0x8083, 0x8237, 0x8418, 0x8624, 0x8805, 0x898e, 0x8b9a, 0x8f87,
    0x900a, 0x926e, 0x947a, 0x96de, 0x9893, 0x9a1c, 0x9b4e, 0x9f92, 0xa041, 0xa24d, 0xa42e, 0xa5e3,
    0xa7ef, 0xa9d0, 0xab59, 0xaf9d, 0xb020, 0xb201, 0xb38a, 0xb56a, 0xb74b, 0xb957, 0xbb38, 0xbfff,
    0xc057, 0xc28f, 0xc51e, 0xc72a, 0xc888, 0xca68, 0xcc75, 0xcfb3, 0xd00a, 0xd168, 0xd2f1, 0xd4a6,
    0xd686, 0xd8be, 0xdacb, 0xdfe9, 0xe041, 0xe221, 0xe3d6, 0xe5e2, 0xe7ef, 0xe978, 0xeb84, 0xeff4,
    0xf077, 0xf40d, 0xf56a, 0xf74b, 0xf9da, 0xfbe6, 0xfe4a, 0xffff,
];

pub const SELECTOR_AMP: u8 = 0x0C;
pub const SELECTOR_OTHER: u8 = 0x0D;

pub fn magic(value: u8) -> u16 {
    MAGIC_VALUES[value.min(127) as usize]
}

/// Parameter indices, Mustang numbering. The SCX2 exposes a subset.
pub fn label(index: u8) -> &'static str {
    match index {
        0x00 => "volume",
        0x01 => "gain",
        0x02 => "gain 2 / blend",
        0x03 => "master",
        0x04 => "treble",
        0x05 => "mid",
        0x06 => "bass",
        0x07 => "presence / cut",
        0x08 => "resonance",
        0x09 => "gate depth",
        0x0A => "bias",
        0x0B => "reserved 0b",
        0x0C => "voice",
        0x0D => "voice",
        0x0E => "voice",
        0x0F => "noise gate",
        0x10 => "gate threshold",
        0x11 => "cabinet",
        0x12 => "power supply",
        0x13 => "sag",
        0x14 => "bright",
        _ => "unknown",
    }
}

pub fn direct_control(model: u16, index: u8, selector: u8, magic: u16) -> [u8; REPORT_SIZE] {
    let mut frame = [0u8; REPORT_SIZE];
    frame[0] = 0x05;
    frame[1] = protocol::HOST_CONTROL;
    frame[2] = 0x02;
    frame[3] = model as u8;
    frame[4] = (model >> 8) as u8;
    frame[5] = index;
    frame[6] = index;
    frame[7] = selector;
    frame[9] = magic as u8;
    frame[10] = (magic >> 8) as u8;
    frame
}

/// Read a 16-bit big-endian parameter pair from a block.
pub fn read_pair(frame: &[u8; REPORT_SIZE], offset: usize) -> u16 {
    u16::from_be_bytes([frame[offset], frame[offset + 1]])
}

/// Block offset of a direct-control parameter, if it has one.
///
/// Verified with `scx2 probe` on hardware: indices 0x00..0x0a map to offsets
/// 32..42, cabinet/sag/bright sit at 49/51/52.
pub fn index_for_offset(offset: usize) -> Option<u8> {
    match offset {
        32..=43 => Some((offset - 32) as u8),
        49 => Some(0x11),
        51 => Some(0x13),
        52 => Some(0x14),
        _ => None,
    }
}

/// Block offset of a direct-control parameter index.
pub fn offset_for_index(index: u8) -> Option<usize> {
    match index {
        0x00..=0x0b => Some(32 + index as usize),
        0x11 => Some(49),
        0x13 => Some(51),
        0x14 => Some(52),
        _ => None,
    }
}

/// The amp stores a parameter byte as `round(magic / 257)`.
pub fn magic_from_byte(value: u8) -> u16 {
    value as u16 * 257
}

/// One knob the editor can offer.
pub struct KnobSpec {
    pub offset: usize,
    pub name: &'static str,
    /// Shown by the original FUSE editor. All but Middle are also on the amp's panel.
    pub standard: bool,
    /// What the parameter seems to do. Hidden entries are educated guesses
    /// based on the Mustang parameter map in `docs/reference/amp.h`.
    pub hint: &'static str,
}

/// The knobs FUSE shows (the amp's five plus Middle), then the parameters that
/// only direct control can reach.
pub const KNOBS: &[KnobSpec] = &[
    KnobSpec {
        offset: 32,
        name: "Volume",
        standard: true,
        hint: "Channel volume, exactly the knob on the amp.",
    },
    KnobSpec {
        offset: 33,
        name: "Gain",
        standard: true,
        hint: "Input gain / preamp drive.",
    },
    KnobSpec {
        offset: 36,
        name: "Treble",
        standard: true,
        hint: "High-frequency tone control.",
    },
    KnobSpec {
        offset: 37,
        name: "Middle",
        standard: true,
        hint: "Midrange tone control. FUSE offers it; the amp's panel has no knob for it.",
    },
    KnobSpec {
        offset: 38,
        name: "Bass",
        standard: true,
        hint: "Low-frequency tone control.",
    },
    KnobSpec {
        offset: 44,
        name: "Voice",
        standard: true,
        hint: "Amp voice 0..12. Channel 2 only: on Channel 1 you hear the clean tube channel.",
    },
    KnobSpec {
        offset: 34,
        name: "Gain 2",
        standard: false,
        hint: "Guessed: second gain stage used by the Supersonic-style voices.",
    },
    KnobSpec {
        offset: 35,
        name: "Master",
        standard: false,
        hint: "Guessed: output-stage level; the SCX2 has no master knob.",
    },
    KnobSpec {
        offset: 39,
        name: "Presence",
        standard: false,
        hint: "Guessed: high-frequency feedback in the power amp.",
    },
    KnobSpec {
        offset: 40,
        name: "Resonance",
        standard: false,
        hint: "FUSE's Resonance control; the SCX2 panel has no knob for it.",
    },
    KnobSpec {
        offset: 41,
        name: "Gate depth",
        standard: false,
        hint: "Noise-gate depth (FUSE calls it NG Depth); also moves the gate pair at 47/48.",
    },
    KnobSpec {
        offset: 42,
        name: "Bias",
        standard: false,
        hint: "Guessed: power-amp bias. Hotter usually means more compression.",
    },
    KnobSpec {
        offset: 47,
        name: "Gate threshold",
        standard: false,
        hint: "Guessed: noise-gate threshold, 16-bit pair with offset 48.",
    },
    KnobSpec {
        offset: 51,
        name: "Sag",
        standard: false,
        hint: "Guessed: power-supply sag; lower is tighter.",
    },
    KnobSpec {
        offset: 52,
        name: "Bright",
        standard: false,
        hint: "Guessed: bright switch, inverted (lower value = brighter).",
    },
];

pub fn knob_spec(offset: usize) -> Option<&'static KnobSpec> {
    KNOBS.iter().find(|spec| spec.offset == offset)
}

/// Display name for a known block offset.
pub fn offset_label(offset: usize) -> Option<&'static str> {
    match offset {
        32 => Some("volume"),
        33 => Some("gain"),
        34 => Some("gain 2"),
        35 => Some("master"),
        36 => Some("treble"),
        37 => Some("middle"),
        38 => Some("bass"),
        39 => Some("presence"),
        40 => Some("resonance"),
        41 => Some("gate depth"),
        42 => Some("bias"),
        44..=46 => Some("voice"),
        47 => Some("gate threshold"),
        49 => Some("cabinet"),
        51 => Some("sag"),
        52 => Some("bright"),
        _ => None,
    }
}

/// Compact name for the parameter grid, where cells are only a few
/// characters wide.
pub fn short_offset_label(offset: usize) -> &'static str {
    match offset {
        32 => "VOL",
        33 => "GAIN",
        34 => "GAIN2",
        35 => "MAST",
        36 => "TREB",
        37 => "MID",
        38 => "BASS",
        39 => "PRES",
        40 => "RES",
        41 => "GATE",
        42 => "BIAS",
        44..=46 => "VOICE",
        47 => "GATE?",
        49 => "CAB",
        51 => "SAG",
        52 => "BRIGHT",
        _ => "",
    }
}

/// Title-case name for a knob caption.
pub fn offset_title(offset: usize) -> Option<String> {
    let label = offset_label(offset)?;
    let mut chars = label.chars();
    let first = chars.next()?.to_ascii_uppercase();
    Some(format!("{first}{}", chars.as_str()))
}
