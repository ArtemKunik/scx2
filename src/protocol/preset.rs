use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::*;

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

    pub fn block_mut(&mut self, kind: Kind) -> Option<&mut Message> {
        self.blocks.get_mut(&kind.code())
    }

    /// The block for `kind`, creating an empty one when the preset does not
    /// carry it yet.
    ///
    /// A stored amp preset has no mod/delay/reverb block until an effect is
    /// loaded, so the editor needs a valid frame to edit. The synthesized frame
    /// has the same header the amp reports; the model and parameters start at
    /// zero. Only blocks that are part of a preset are created.
    pub fn ensure_block(&mut self, kind: Kind) -> Option<&mut Message> {
        if !kind.has_model() || !kind.is_preset_scoped() {
            return None;
        }
        if !self.blocks.contains_key(&kind.code()) {
            let mut raw = [0u8; REPORT_SIZE];
            raw[0] = FRAME_MARKER;
            raw[1] = AMP_TO_HOST;
            raw[2] = kind.code();
            raw[3] = self.bank;
            raw[4] = self.index;
            raw[6] = 0x01;
            if kind.has_slot() {
                raw[OFF_SLOT] = default_slot(kind);
            }
            let message = Message::from_bytes(raw)?;
            self.blocks.insert(kind.code(), message);
        }
        self.blocks.get_mut(&kind.code())
    }

    /// Load a DSP model into a block, or clear it with model id `0`.
    ///
    /// Returns false for kinds that do not carry a model. Existing parameters
    /// are kept; use [`Preset::load_model`] when changing from one effect to
    /// another, so the new model does not inherit the old one's settings.
    pub fn set_model(&mut self, kind: Kind, model: u16) -> bool {
        let Some(block) = self.ensure_block(kind) else {
            return false;
        };
        let [low, high] = model.to_le_bytes();
        block.set_byte(OFF_MODEL, low);
        block.set_byte(OFF_MODEL + 1, high);
        true
    }

    /// Load a model and give it its own parameter defaults.
    ///
    /// FUSE knows the factory defaults for many effects; for the rest this uses
    /// a neutral `0x80` in every used slot. Clearing (`model == 0`) zeroes the
    /// parameter bytes. Amp models also move the voice bytes at 44..46.
    pub fn load_model(&mut self, kind: Kind, model: u16) -> bool {
        let Some(block) = self.ensure_block(kind) else {
            return false;
        };
        let [low, high] = model.to_le_bytes();
        block.set_byte(OFF_MODEL, low);
        block.set_byte(OFF_MODEL + 1, high);

        if model == 0 {
            for offset in OFF_PARAMS..REPORT_SIZE {
                block.set_byte(offset, 0);
            }
            return true;
        }

        if kind == Kind::Amp {
            if let Some(voice) = crate::models::voice_index(model) {
                for offset in 44..=46 {
                    block.set_byte(offset, voice);
                }
            }
            return true;
        }

        if let Some(defaults) = crate::models::default_params(kind, model) {
            for (index, value) in defaults.iter().enumerate() {
                block.set_byte(OFF_PARAMS + index, *value);
            }
        }
        true
    }

    /// Set the preset name, updating (or creating) the `0x04` name block.
    ///
    /// Names are 32 bytes, NUL padded. The block keeps the preset's bank and
    /// index so it can be sent back to the same slot.
    pub fn set_name(&mut self, name: &str) {
        self.name = name.chars().take(NAME_LEN).collect();
        let bytes = self.name.as_bytes();

        if self.name_message.is_none() {
            let mut raw = [0u8; REPORT_SIZE];
            raw[0] = FRAME_MARKER;
            raw[1] = AMP_TO_HOST;
            raw[2] = Kind::Name.code();
            raw[3] = self.bank;
            raw[4] = self.index;
            raw[6] = 0x01;
            if let Some(message) = Message::from_bytes(raw) {
                self.name_message = Some(message);
            }
        }
        if let Some(message) = self.name_message.as_mut() {
            for (index, slot) in (OFF_NAME..OFF_NAME + NAME_LEN).enumerate() {
                message.set_byte(slot, bytes.get(index).copied().unwrap_or(0));
            }
        }
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

/// The slot byte observed for a freshly created effect block.
///
/// The amp reuses this byte for the effect's position in the chain, so a
/// synthesized block only needs a plausible value until the amp reports its
/// own layout back.
fn default_slot(kind: Kind) -> u8 {
    match kind {
        Kind::Stomp => 3,
        Kind::Delay => 1,
        Kind::Reverb => 1,
        _ => 0,
    }
}
