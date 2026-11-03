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
