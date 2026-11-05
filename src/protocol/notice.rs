//! Reports the amp sends on its own, and its answers to direct control.
//!
//! Captured on hardware while turning the front-panel knobs (fw `May 30 2013`):
//!
//! ```text
//! 05 01 02 53 00 00 00 0c 00 2d 2d   amp parameter 0 (volume) is now 0x2d
//! 00 00 05 00 ...                    direct-control write accepted, status 0
//! 1d 01 01 06 ...                    selector knob position
//! ```

use super::REPORT_SIZE;

/// Selector byte of an amp parameter, in both directions.
const SELECTOR_AMP: u8 = 0x0C;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// An amp parameter moved. `index` uses direct-control numbering.
    AmpParam { model: u16, index: u8, value: u8 },
    /// An effect parameter moved. `module` is the amp's effect-module byte
    /// (5 and 6 were observed for delay and reverb), `model` is the DSP model,
    /// and `index` is the parameter slot at block offset `32 + index`.
    EffectParam {
        module: u8,
        model: u16,
        index: u8,
        value: u8,
    },
    /// A selector knob position, sent at the end of a dump.
    KnobPosition { index: u8, position: u8 },
    /// The `1f 01` state summary sent before a dump.
    Summary,
    /// Answer to a direct-control write; status 0 means accepted.
    Ack { status: u8 },
    /// Something else changed (voice, effects). The amp does not say what, so
    /// the block has to be read again.
    StateChanged,
}

/// Recognise a report that is not part of a dump's block stream.
pub fn classify(report: &[u8; REPORT_SIZE]) -> Option<Notice> {
    match (report[0], report[1]) {
        (0x05, 0x01) => {
            let repeated = report[5] == report[6];
            let model = u16::from_le_bytes([report[3], report[4]]);
            let value = byte_from_wire(u16::from_le_bytes([report[9], report[10]]));
            if report[2] == 0x02 && repeated && report[7] == SELECTOR_AMP {
                Some(Notice::AmpParam {
                    model,
                    index: report[5],
                    value,
                })
            } else if repeated && report[7] == 0x01 {
                // Seen at the end of every dump: one report per effect module,
                // carrying the same 16-bit value shape as direct control.
                Some(Notice::EffectParam {
                    module: report[2],
                    model,
                    index: report[5],
                    value,
                })
            } else {
                Some(Notice::StateChanged)
            }
        }
        (0x1d, 0x01) => Some(Notice::KnobPosition {
            index: report[2],
            position: report[3],
        }),
        (0x1f, 0x01) => Some(Notice::Summary),
        (0x00, _) if report[2] == 0x05 => Some(Notice::Ack { status: report[3] }),
        (0x13 | 0x19 | 0x1B | 0x20, 0x01) => Some(Notice::StateChanged),
        _ => None,
    }
}

/// The block stores a parameter as `round(wire / 257)`.
fn byte_from_wire(wire: u16) -> u8 {
    ((u32::from(wire) + 128) / 257) as u8
}
