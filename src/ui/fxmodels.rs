//! The face catalogue: which enclosure, colour and legends each model gets.

use eframe::egui::Color32;

use crate::models;
use crate::protocol::Kind;

use super::fx::{Form, Pedal};

const DRIVE: &[&str] = &["LEVEL", "TONE", "DRIVE"];
const MOD: &[&str] = &["RATE", "DEPTH", "MIX"];
const DELAY: &[&str] = &["TIME", "FEEDBACK", "MIX"];
const REVERB: &[&str] = &["DWELL", "MIX", "TONE"];
const RACK: &[&str] = &["TIME", "MIX"];
const COMP: &[&str] = &["LEVEL", "SUSTAIN"];
const WAH: &[&str] = &[];
const TAPE: &[&str] = &["TIME", "REPEATS", "MIX"];

const CHROME: Color32 = Color32::from_rgb(176, 178, 180);
const BLACK_KNOB: Color32 = Color32::from_rgb(28, 28, 30);
const WHITE_INK: Color32 = Color32::from_rgb(238, 236, 230);
#[allow(clippy::too_many_arguments)]
fn pedal(
    form: Form,
    body: Color32,
    edge: Color32,
    face: Color32,
    ink: Color32,
    knobs: usize,
    knob: Color32,
    knob_cap: Color32,
    led: Color32,
    accent: Color32,
    name: &'static str,
    sub: &'static str,
    labels: &'static [&'static str],
) -> Pedal {
    Pedal {
        form,
        body,
        edge,
        face,
        ink,
        knobs,
        knob,
        knob_cap,
        led,
        accent,
        brand: "FENDER",
        name,
        sub,
        labels,
    }
}

fn green() -> Pedal {
    pedal(
        Form::Box,
        Color32::from_rgb(48, 106, 60),
        Color32::from_rgb(20, 46, 26),
        Color32::from_rgb(60, 126, 72),
        WHITE_INK,
        3,
        BLACK_KNOB,
        CHROME,
        Color32::from_rgb(180, 255, 120),
        Color32::from_rgb(150, 206, 158),
        "GREEN BOX",
        "OVERDRIVE",
        DRIVE,
    )
}

fn orange() -> Pedal {
    pedal(
        Form::Box,
        Color32::from_rgb(188, 88, 26),
        Color32::from_rgb(84, 36, 8),
        Color32::from_rgb(214, 112, 40),
        Color32::from_rgb(38, 20, 8),
        3,
        BLACK_KNOB,
        CHROME,
        Color32::from_rgb(255, 196, 90),
        Color32::from_rgb(246, 196, 138),
        "ORANGE BOX",
        "DISTORTION",
        DRIVE,
    )
}

fn dark_box(
    name: &'static str,
    sub: &'static str,
    led: Color32,
    labels: &'static [&'static str],
) -> Pedal {
    pedal(
        Form::Box,
        Color32::from_rgb(36, 36, 40),
        Color32::from_rgb(12, 12, 14),
        Color32::from_rgb(52, 52, 58),
        WHITE_INK,
        3,
        BLACK_KNOB,
        CHROME,
        led,
        Color32::from_rgb(156, 158, 164),
        name,
        sub,
        labels,
    )
}

fn colored(
    body: [u8; 3],
    name: &'static str,
    sub: &'static str,
    knobs: usize,
    labels: &'static [&'static str],
) -> Pedal {
    let edge = Color32::from_rgb(body[0] / 2, body[1] / 2, body[2] / 2);
    pedal(
        Form::Box,
        Color32::from_rgb(body[0], body[1], body[2]),
        edge,
        Color32::from_rgb(
            (body[0] as u16 + 22).min(255) as u8,
            (body[1] as u16 + 22).min(255) as u8,
            (body[2] as u16 + 22).min(255) as u8,
        ),
        WHITE_INK,
        knobs,
        BLACK_KNOB,
        CHROME,
        Color32::from_rgb(255, 196, 90),
        Color32::from_rgb(
            (body[0] as u16 + 96).min(255) as u8,
            (body[1] as u16 + 96).min(255) as u8,
            (body[2] as u16 + 96).min(255) as u8,
        ),
        name,
        sub,
        labels,
    )
}

fn silver(
    name: &'static str,
    sub: &'static str,
    knobs: usize,
    labels: &'static [&'static str],
) -> Pedal {
    pedal(
        Form::Box,
        Color32::from_rgb(178, 180, 182),
        Color32::from_rgb(88, 90, 92),
        Color32::from_rgb(208, 210, 212),
        Color32::from_rgb(32, 32, 36),
        knobs,
        BLACK_KNOB,
        CHROME,
        Color32::from_rgb(120, 255, 140),
        Color32::from_rgb(64, 66, 70),
        name,
        sub,
        labels,
    )
}

fn wah(name: &'static str) -> Pedal {
    pedal(
        Form::Wah,
        Color32::from_rgb(26, 26, 28),
        Color32::from_rgb(8, 8, 9),
        Color32::from_rgb(52, 52, 56),
        WHITE_INK,
        0,
        BLACK_KNOB,
        CHROME,
        Color32::from_rgb(255, 70, 50),
        Color32::from_rgb(196, 198, 202),
        name,
        "TREADLE",
        WAH,
    )
}

fn tape() -> Pedal {
    pedal(
        Form::Tape,
        Color32::from_rgb(198, 188, 160),
        Color32::from_rgb(106, 98, 76),
        Color32::from_rgb(38, 36, 32),
        Color32::from_rgb(42, 34, 20),
        3,
        BLACK_KNOB,
        CHROME,
        Color32::from_rgb(255, 170, 60),
        Color32::from_rgb(150, 136, 104),
        "TAPE ECHO",
        "DELAY",
        TAPE,
    )
}

fn tank(name: &'static str) -> Pedal {
    pedal(
        Form::Tank,
        Color32::from_rgb(88, 58, 38),
        Color32::from_rgb(38, 24, 14),
        Color32::from_rgb(176, 178, 182),
        Color32::from_rgb(26, 24, 22),
        3,
        BLACK_KNOB,
        CHROME,
        Color32::from_rgb(255, 70, 50),
        Color32::from_rgb(150, 150, 150),
        name,
        "SPRING REVERB",
        REVERB,
    )
}

fn rack(name: &'static str, led: Color32) -> Pedal {
    pedal(
        Form::Rack,
        Color32::from_rgb(26, 28, 32),
        Color32::from_rgb(8, 9, 10),
        Color32::from_rgb(152, 154, 158),
        Color32::from_rgb(28, 30, 34),
        2,
        BLACK_KNOB,
        CHROME,
        led,
        Color32::from_rgb(90, 94, 100),
        name,
        "PROCESSOR",
        RACK,
    )
}

/// The hardware for a loaded model, if the id is known.
pub(super) fn look(kind: Kind, model: u16) -> Option<Pedal> {
    let fallback = models::model_name(kind, model).unwrap_or("EFFECT");
    let mut pedal = match (kind, model) {
        (Kind::Stomp, 0x003c) => colored([92, 78, 26], "OVERDRIVE", "DRIVE", 3, DRIVE),
        (Kind::Stomp, 0x0049 | 0x004a | 0x001c) => wah("WAH"),
        (Kind::Stomp, 0x001a) => colored([138, 30, 26], "FUZZ", "FUZZ", 2, COMP),
        (Kind::Stomp, 0x0088 | 0x0007) => silver("COMPRESSOR", "DYNAMICS", 2, COMP),
        (Kind::Stomp, 0x0103) => silver("RANGER", "BOOST", 1, COMP),
        (Kind::Stomp, 0x00ba) => green(),
        (Kind::Stomp, 0x0110) => orange(),
        (Kind::Stomp, 0x0111) => dark_box(
            "BLACK BOX",
            "DISTORTION",
            Color32::from_rgb(255, 80, 60),
            DRIVE,
        ),
        (Kind::Stomp, 0x010f) => pedal(
            Form::Box,
            Color32::from_rgb(206, 198, 176),
            Color32::from_rgb(108, 102, 88),
            Color32::from_rgb(30, 30, 32),
            WHITE_INK,
            3,
            BLACK_KNOB,
            CHROME,
            Color32::from_rgb(255, 210, 80),
            Color32::from_rgb(150, 142, 120),
            "BIG FUZZ",
            "FUZZ",
            DRIVE,
        ),
        (Kind::Mod, 0x0012 | 0x0013) => colored([40, 82, 148], "CHORUS", "MODULATION", 3, MOD),
        (Kind::Mod, 0x0018 | 0x0019) => colored([92, 54, 140], "FLANGER", "MODULATION", 3, MOD),
        (Kind::Mod, 0x002d) => colored([32, 44, 76], "VIBRATONE", "ROTARY", 1, MOD),
        (Kind::Mod, 0x0040 | 0x0041) => dark_box(
            "TREMOLO",
            "MODULATION",
            Color32::from_rgb(255, 190, 70),
            MOD,
        ),
        (Kind::Mod, 0x0022) => colored([176, 144, 36], "RING MOD", "MODULATION", 2, MOD),
        (Kind::Mod, 0x0029) => silver("STEP FILTER", "FILTER", 3, MOD),
        (Kind::Mod, 0x004f) => colored([204, 112, 34], "PHASER", "MODULATION", 2, MOD),
        (Kind::Mod, 0x001f | 0x101f) => {
            dark_box("PITCH", "SHIFTER", Color32::from_rgb(90, 170, 255), MOD)
        }
        (Kind::Mod, 0x00f4 | 0x00f5) => wah("MOD WAH"),
        (Kind::Delay, 0x002b | 0x002a) => tape(),
        (Kind::Delay, 0x0043 | 0x0048 | 0x0044 | 0x0045) => {
            rack(fallback, Color32::from_rgb(90, 220, 130))
        }
        (Kind::Delay, _) => colored([74, 92, 82], "DELAY", "DELAY", 3, DELAY),
        (Kind::Reverb, 0x0021 | 0x000b) => tank(fallback),
        (Kind::Reverb, _) => rack(fallback, Color32::from_rgb(90, 190, 255)),
        _ => return None,
    };

    // The artwork decides the enclosure; FUSE's model table decides what the
    // knobs actually control. A treadle has no knobs of its own, so its
    // parameters live in the effect panel instead.
    if pedal.form != Form::Wah {
        let labels = models::effect_params(kind, model);
        if !labels.is_empty() {
            pedal.labels = labels;
            pedal.knobs = models::effect_knobs(kind, model);
        }
    }
    Some(pedal)
}
