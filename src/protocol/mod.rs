//! Wire protocol for the Fender Super Champ X2.
//!
//! The amp is a composite USB device:
//!
//! ```text
//! VID 0x1ED8  PID 0x000E
//!   interface 0  HID, interrupt IN 0x81 / OUT 0x01, 64-byte reports  <- this module
//!   interface 2  USB audio (the recording interface)
//! ```
//!
//! FUSE talked to interface 0 through hidapi. Everything below was recovered
//! from a real FUSE session against an SCX2, see `docs/protocol.md`.
//!
//! Frame layout of an amp -> host state report:
//!
//! ```text
//!  0     0x1c                 frame marker
//!  1     0x01                 amp -> host
//!  2     kind                 see [`Kind`]
//!  3     bank                 0 live edit buffer, 1 amp presets, 2 fx presets
//!  4     index                slot within the bank
//!  5     -                    observed 0
//!  6     0x01                 always set in observed frames
//!  7     flag                 kind dependent (1 on amp/master blocks)
//!  16..18  model id (LE u16)  shared with the Mustang model id space
//!  18      fx slot            effect slot the block is loaded into
//!  32..64  parameters         8-bit, 0x80 = centre for bipolar values
//! ```

pub use notice::{classify, Notice};

pub const VENDOR_ID: u16 = 0x1ED8;
pub const PRODUCT_ID: u16 = 0x000E;

/// Every report is exactly this long; the HID endpoints declare 64 bytes.
pub const REPORT_SIZE: usize = 64;

/// First two bytes of every amp -> host report.
pub const FRAME_MARKER: u8 = 0x1C;
pub const AMP_TO_HOST: u8 = 0x01;
/// Direction byte FUSE uses for handshake and requests.
pub const HOST_REQUEST: u8 = 0xC1;
/// Direction byte FUSE uses for the Mustang direct-control frames.
pub const HOST_CONTROL: u8 = 0xC3;

pub const OFF_MODEL: usize = 16;
pub const OFF_SLOT: usize = 18;
pub const OFF_NAME: usize = 16;
pub const OFF_PARAMS: usize = 32;
pub const NAME_LEN: usize = 32;

/// Bank numbers as reported in byte 3.
pub const BANK_LIVE: u8 = 0;
pub const BANK_AMP: u8 = 1;
pub const BANK_FX: u8 = 2;

/// The amp's end-of-dump acknowledgement.
pub const END_OF_DUMP: [u8; 2] = [0xFF, 0x01];
mod diff;
mod dump;
mod frames;
mod kind;
mod message;
mod notice;
mod preset;

pub use diff::{diff_dumps, diff_presets, Change};
pub use dump::Dump;
pub use frames::{
    as_host_frame, boot_banner, execute_model_change, handshake_step1, handshake_step2, hex,
    name_change, parse_hex, patch_change, recall_preset, request_dump, retarget, save_preset,
};
pub use kind::Kind;
pub use message::Message;
pub use preset::Preset;
