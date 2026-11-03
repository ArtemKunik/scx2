use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::*;

/// Result of a full parameter dump.
#[derive(Debug, Clone, Default)]
pub struct Dump {
    pub(crate) presets: BTreeMap<(u8, u8), Preset>,
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

    /// A dump holding a single preset, for files that carry one preset (`.fuse`).
    pub fn single(preset: Preset) -> Self {
        let mut dump = Dump::new();
        dump.presets.insert((preset.bank, preset.index), preset);
        dump
    }

    /// Add or replace a preset.
    pub fn insert(&mut self, preset: Preset) {
        self.presets.insert((preset.bank, preset.index), preset);
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

    pub fn get_mut(&mut self, bank: u8, index: u8) -> Option<&mut Preset> {
        self.presets.get_mut(&(bank, index))
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
