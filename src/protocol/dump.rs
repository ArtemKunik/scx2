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
        // Library metadata travels beside the device bytes, so attach it once
        // the presets have been rebuilt from their raw reports.
        for preset in presets {
            let (Some(bank), Some(index)) = (
                preset["bank"]
                    .as_u64()
                    .and_then(|bank| u8::try_from(bank).ok()),
                preset["index"]
                    .as_u64()
                    .and_then(|index| u8::try_from(index).ok()),
            ) else {
                continue;
            };
            let info = PresetInfo::from_json(&preset["info"]);
            if info.is_empty() {
                continue;
            }
            if let Some(target) = dump.get_mut(bank, index) {
                target.info = info;
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

    /// The live buffer's `0x0D` misc report, which is not tied to a preset.
    pub fn live_misc(&self) -> Option<&Message> {
        self.extras
            .iter()
            .find(|message| message.kind() == Kind::Misc && message.bank() == BANK_LIVE)
    }

    /// The amp's USB recording level, read from the live misc block.
    pub fn usb_gain(&self) -> Option<u8> {
        self.live_misc()
            .map(|message| message.byte(USB_GAIN_OFFSET))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn misc(bank: u8, gain: u8) -> [u8; REPORT_SIZE] {
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = Kind::Misc.code();
        raw[3] = bank;
        raw[4] = USB_GAIN_OFFSET as u8;
        raw[6] = 1;
        raw[USB_GAIN_OFFSET] = gain;
        raw
    }

    #[test]
    fn usb_gain_comes_from_the_live_misc_block() {
        let dump = Dump::from_reports([misc(BANK_LIVE, 0x40)]);
        assert_eq!(dump.usb_gain(), Some(0x40));
    }

    #[test]
    fn a_stored_slot_misc_block_is_not_the_live_setting() {
        let dump = Dump::from_reports([misc(BANK_AMP, 0x40)]);
        assert!(dump.live_misc().is_none());
        assert_eq!(dump.usb_gain(), None);
    }

    #[test]
    fn preset_info_round_trips_without_touching_the_blocks() {
        let mut raw = [0u8; REPORT_SIZE];
        raw[0] = FRAME_MARKER;
        raw[1] = AMP_TO_HOST;
        raw[2] = Kind::Amp.code();
        raw[3] = BANK_AMP;
        raw[4] = 3;
        raw[6] = 1;
        raw[OFF_MODEL] = 0x64;
        raw[OFF_PARAMS] = 0x80;

        let mut preset = Preset::new(BANK_AMP, 3);
        preset.set_name("Tweed Bassman");
        preset.add(Message::from_bytes(raw).expect("a valid amp block"));
        let block = *preset.block(Kind::Amp).expect("the amp block").as_bytes();
        preset.info = PresetInfo {
            description: "Edges of breakup".into(),
            tags: vec!["blues".into(), "clean".into()],
            genre: "Blues".into(),
            sub_genres: ["Chicago".into(), "Modern".into()],
            author: "A. Player".into(),
            rating: 4,
            comments: "A favourite".into(),
        };
        let info = preset.info.clone();

        let mut dump = Dump::new();
        dump.insert(preset);
        let text = serde_json::to_string(&dump.to_json()).expect("a backup");
        let back = Dump::from_json(&text).expect("the backup loads");

        let reloaded = back.get(BANK_AMP, 3).expect("the slot");
        assert_eq!(reloaded.info, info);
        assert_eq!(
            reloaded.block(Kind::Amp).expect("the amp block").as_bytes(),
            &block,
            "metadata must not move a device byte"
        );
    }

    #[test]
    fn an_empty_info_is_not_written_to_the_backup() {
        let preset = Preset::new(BANK_AMP, 3);
        assert!(preset.to_json().get("info").is_none());
    }
}
