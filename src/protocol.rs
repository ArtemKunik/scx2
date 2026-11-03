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

use std::collections::BTreeMap;

use serde_json::{json, Value};

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

/// One decoded 64-byte state report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    raw: [u8; REPORT_SIZE],
}

impl Message {
    /// Wrap a raw report, if it is an amp state frame.
    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() != REPORT_SIZE || raw[0] != FRAME_MARKER || raw[1] != AMP_TO_HOST {
            return None;
        }
        let mut buf = [0u8; REPORT_SIZE];
        buf.copy_from_slice(raw);
        Some(Message { raw: buf })
    }

    pub fn from_bytes(raw: [u8; REPORT_SIZE]) -> Option<Self> {
        Self::parse(&raw)
    }

    pub fn as_bytes(&self) -> &[u8; REPORT_SIZE] {
        &self.raw
    }

    pub fn byte(&self, offset: usize) -> u8 {
        self.raw[offset]
    }

    /// Overwrite one byte. Used by the editor and by the tests.
    pub fn set_byte(&mut self, offset: usize, value: u8) {
        self.raw[offset] = value;
    }

    pub fn kind(&self) -> Kind {
        Kind::from_code(self.raw[2])
    }

    pub fn bank(&self) -> u8 {
        self.raw[3]
    }

    pub fn index(&self) -> u8 {
        self.raw[4]
    }

    pub fn flag(&self) -> u8 {
        self.raw[7]
    }

    pub fn model(&self) -> Option<u16> {
        self.kind()
            .has_model()
            .then(|| u16::from_le_bytes([self.raw[OFF_MODEL], self.raw[OFF_MODEL + 1]]))
    }

    pub fn slot(&self) -> Option<u8> {
        self.kind().has_slot().then(|| self.raw[OFF_SLOT])
    }

    pub fn params(&self) -> &[u8] {
        &self.raw[OFF_PARAMS..]
    }

    pub fn name(&self) -> String {
        if self.kind() != Kind::Name {
            return String::new();
        }
        let field = &self.raw[OFF_NAME..OFF_NAME + NAME_LEN];
        let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        String::from_utf8_lossy(&field[..end]).trim().to_string()
    }

    /// Human readable one-line summary.
    pub fn summary(&self) -> String {
        let mut out = format!(
            "{} bank={} idx={}",
            self.kind().label(),
            self.bank(),
            self.index()
        );
        if self.kind() == Kind::Name {
            out.push_str(&format!(" name={:?}", self.name()));
        } else if let Some(model) = self.model() {
            out.push_str(&format!(" model=0x{model:04x}"));
            if let Some(slot) = self.slot() {
                out.push_str(&format!(" slot={slot}"));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// host -> amp commands
// ---------------------------------------------------------------------------

fn report(bytes: &[u8]) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    let n = bytes.len().min(REPORT_SIZE);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf
}

/// First message FUSE sends after opening the device.
pub fn handshake_step1() -> [u8; REPORT_SIZE] {
    report(&[0x00, HOST_CONTROL])
}

/// Second message; `0xc1` selects the v2/SCX2 command dialect.
pub fn handshake_step2() -> [u8; REPORT_SIZE] {
    report(&[0x1A, HOST_REQUEST])
}

/// Ask the amp for a full parameter dump. It answers with a stream of state
/// reports and closes with [`END_OF_DUMP`].
pub fn request_dump() -> [u8; REPORT_SIZE] {
    report(&[0xFF, HOST_REQUEST])
}

/// Recall a stored preset. Mirrors the Mustang patch-change frame; not yet
/// verified on SCX2 hardware, kept for the probe tools.
pub fn patch_change(slot: u8) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = FRAME_MARKER;
    buf[1] = AMP_TO_HOST;
    buf[2] = 0x01;
    buf[4] = slot;
    buf[6] = 0x01;
    buf
}

/// The Mustang "execute" frame, sent after a model/preset frame.
pub fn execute_model_change() -> [u8; REPORT_SIZE] {
    report(&[0x1C, 0x03])
}

/// Turn an amp report into a host report, keeping the payload untouched.
pub fn as_host_frame(raw: &[u8; REPORT_SIZE], direction: u8) -> [u8; REPORT_SIZE] {
    let mut buf = *raw;
    buf[1] = direction;
    buf
}

/// Parse a list of hex bytes, e.g. `"1c 01 05 ff"` or `"1c0105ff"`.
pub fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != ',')
        .collect();
    if !cleaned.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
        .collect();
    bytes
}

/// The two reports the amp sends during the handshake carry a build banner,
/// e.g. `May 30 2013`, at offset 4.
pub fn boot_banner(raw: &[u8]) -> Option<String> {
    let field = raw.get(4..18)?;
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let text = String::from_utf8_lossy(&field[..end]).trim().to_string();
    text.chars()
        .next()
        .filter(char::is_ascii_alphabetic)
        .map(|_| text)
}

// ---------------------------------------------------------------------------
// dump model
// ---------------------------------------------------------------------------

/// A preset assembled from its per-block reports.
#[derive(Debug, Clone, Default)]
pub struct Preset {
    pub bank: u8,
    pub index: u8,
    pub name: String,
    name_message: Option<Message>,
    /// Keyed by kind code so the output order is stable.
    pub blocks: BTreeMap<u8, Message>,
}

impl Preset {
    pub fn new(bank: u8, index: u8) -> Self {
        Preset {
            bank,
            index,
            ..Default::default()
        }
    }

    pub fn add(&mut self, message: Message) {
        if message.kind() == Kind::Name {
            self.name = message.name();
            self.name_message = Some(message);
        } else {
            self.blocks.insert(message.kind().code(), message);
        }
    }

    pub fn block(&self, kind: Kind) -> Option<&Message> {
        self.blocks.get(&kind.code())
    }

    pub fn name_message(&self) -> Option<&Message> {
        self.name_message.as_ref()
    }

    pub fn amp_model(&self) -> Option<u16> {
        self.block(Kind::Amp).and_then(Message::model)
    }

    /// Every report that makes up this preset, name first.
    pub fn messages(&self) -> Vec<&Message> {
        let mut out: Vec<&Message> = self.name_message.iter().collect();
        out.extend(self.blocks.values());
        out
    }

    pub fn to_json(&self) -> Value {
        let mut blocks = serde_json::Map::new();
        for message in self.messages() {
            if message.kind() == Kind::Name {
                continue;
            }
            let kind = message.kind();
            let mut entry = serde_json::Map::new();
            if let Some(model) = message.model() {
                entry.insert("model".into(), json!(format!("0x{model:04x}")));
                if let Some(name) = crate::models::model_name(kind, model) {
                    entry.insert("model_name".into(), json!(name));
                }
            }
            if let Some(slot) = message.slot() {
                entry.insert("slot".into(), json!(slot));
            }
            entry.insert("params_hex".into(), json!(hex(message.params())));
            entry.insert("raw_hex".into(), json!(hex(message.as_bytes())));
            blocks.insert(kind.label().to_string(), Value::Object(entry));
        }
        json!({
            "bank": self.bank,
            "index": self.index,
            "name": self.name,
            "name_raw_hex": self.name_message.as_ref().map(|m| hex(m.as_bytes())),
            "blocks": Value::Object(blocks),
        })
    }
}

/// Result of a full parameter dump.
#[derive(Debug, Clone, Default)]
pub struct Dump {
    presets: BTreeMap<(u8, u8), Preset>,
    /// Reports that are not tied to a preset slot (pedal, live misc).
    pub extras: Vec<Message>,
    pub unparsed: Vec<[u8; REPORT_SIZE]>,
    pub ended: bool,
}

impl Dump {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, message: Message) {
        // Stored presets carry a `misc` block at their own index; in the live
        // buffer the same kind reuses byte 4 for something else.
        let scoped = message.kind().is_preset_scoped()
            || (message.kind() == Kind::Misc && message.bank() != BANK_LIVE);
        if !scoped {
            self.extras.push(message);
            return;
        }
        let key = (message.bank(), message.index());
        let preset = self
            .presets
            .entry(key)
            .or_insert_with(|| Preset::new(key.0, key.1));
        preset.add(message);
    }

    pub fn add_raw(&mut self, raw: &[u8]) {
        if let Some(message) = Message::parse(raw) {
            self.add(message);
        } else if raw.starts_with(&END_OF_DUMP) {
            self.ended = true;
        } else if raw.len() == REPORT_SIZE {
            let mut buf = [0u8; REPORT_SIZE];
            buf.copy_from_slice(raw);
            self.unparsed.push(buf);
        }
    }

    pub fn from_reports<I, B>(reports: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: AsRef<[u8]>,
    {
        let mut dump = Dump::new();
        for raw in reports {
            dump.add_raw(raw.as_ref());
        }
        dump
    }

    /// Rebuild a dump from the JSON written by [`Dump::to_json`].
    ///
    /// Everything is reconstructed from the stored `raw_hex`, so a backup is
    /// byte-for-byte lossless.
    pub fn from_json(text: &str) -> serde_json::Result<Self> {
        let value: Value = serde_json::from_str(text)?;
        let mut dump = Dump::new();
        dump.ended = value["ended"].as_bool().unwrap_or(false);
        let Some(presets) = value["presets"].as_array() else {
            return Ok(dump);
        };
        for preset in presets {
            let mut feed = |hex_text: &str| {
                if let Some(bytes) = parse_hex(hex_text) {
                    if bytes.len() >= REPORT_SIZE {
                        dump.add_raw(&bytes[..REPORT_SIZE]);
                    }
                }
            };
            if let Some(name) = preset["name_raw_hex"].as_str() {
                feed(name);
            }
            if let Some(blocks) = preset["blocks"].as_object() {
                for block in blocks.values() {
                    if let Some(raw) = block["raw_hex"].as_str() {
                        feed(raw);
                    }
                }
            }
        }
        Ok(dump)
    }

    pub fn presets(&self) -> impl Iterator<Item = &Preset> {
        self.presets.values()
    }

    pub fn bank(&self, bank: u8) -> impl Iterator<Item = &Preset> {
        self.presets.values().filter(move |p| p.bank == bank)
    }

    pub fn live(&self) -> Option<&Preset> {
        self.presets.get(&(BANK_LIVE, 0))
    }

    pub fn get(&self, bank: u8, index: u8) -> Option<&Preset> {
        self.presets.get(&(bank, index))
    }

    pub fn to_json(&self) -> Value {
        json!({
            "device": "Fender Super Champ X2",
            "vendor_id": format!("0x{VENDOR_ID:04x}"),
            "product_id": format!("0x{PRODUCT_ID:04x}"),
            "ended": self.ended,
            "presets": self.presets().map(Preset::to_json).collect::<Vec<_>>(),
            "extras": self.extras.iter().map(|m| {
                json!({
                    "kind": m.kind().label(),
                    "index": m.index(),
                    "raw_hex": hex(m.as_bytes()),
                })
            }).collect::<Vec<_>>(),
            "unparsed_hex": self.unparsed.iter().map(|r| hex(r)).collect::<Vec<_>>(),
        })
    }
}

// ---------------------------------------------------------------------------
// diffing
// ---------------------------------------------------------------------------

/// One byte that differs between two versions of the same block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub bank: u8,
    pub index: u8,
    pub kind: Kind,
    pub offset: usize,
    pub before: u8,
    pub after: u8,
}

impl Change {
    /// `amp bank=0 idx=0 offset 35: 0x80 -> 0x63`
    pub fn line(&self) -> String {
        format!(
            "{:<7} bank={} idx={} offset {:>2}: 0x{:02x} -> 0x{:02x}",
            self.kind.label(),
            self.bank,
            self.index,
            self.offset,
            self.before,
            self.after
        )
    }
}

/// Byte-level differences between two versions of one preset.
pub fn diff_presets(a: &Preset, b: &Preset) -> Vec<Change> {
    let mut changes = Vec::new();
    let kinds: std::collections::BTreeSet<u8> =
        a.blocks.keys().chain(b.blocks.keys()).copied().collect();
    let empty = [0u8; REPORT_SIZE];
    for code in kinds {
        let before = a.blocks.get(&code).map(Message::as_bytes).unwrap_or(&empty);
        let after = b.blocks.get(&code).map(Message::as_bytes).unwrap_or(&empty);
        for offset in 0..REPORT_SIZE {
            if before[offset] != after[offset] {
                changes.push(Change {
                    bank: b.bank,
                    index: b.index,
                    kind: Kind::from_code(code),
                    offset,
                    before: before[offset],
                    after: after[offset],
                });
            }
        }
    }
    changes
}

/// Byte-level differences between two dumps, preset by preset.
pub fn diff_dumps(a: &Dump, b: &Dump) -> Vec<Change> {
    let keys: std::collections::BTreeSet<(u8, u8)> =
        a.presets.keys().chain(b.presets.keys()).copied().collect();
    let mut changes = Vec::new();
    for key in keys {
        match (a.presets.get(&key), b.presets.get(&key)) {
            (Some(before), Some(after)) => changes.extend(diff_presets(before, after)),
            (None, Some(after)) => {
                let empty = Preset::new(after.bank, after.index);
                changes.extend(diff_presets(&empty, after));
            }
            (Some(before), None) => {
                let empty = Preset::new(before.bank, before.index);
                changes.extend(diff_presets(before, &empty));
            }
            (None, None) => {}
        }
    }
    changes
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
