//! Fender FUSE `.fuse` preset files.
//!
//! A `.fuse` file is a small XML document describing one preset:
//!
//! ```xml
//! <Preset name="Tweed">
//!   <Amplifier>
//!     <Module ID="0" POS="0" BypassState="0">
//!       <Param ControlIndex="0">43690</Param>
//!     </Module>
//!   </Amplifier>
//!   <FX>
//!     <Stompbox>
//!       <Module ID="19" POS="3" BypassState="0">...</Module>
//!     </Stompbox>
//!   </FX>
//! </Preset>
//! ```
//!
//! `ID` is FUSE's model numbering, not the DSP model id; [`crate::models`]
//! maps between them. Parameter values are 16-bit and FUSE stores the same
//! byte twice (`byte * 257`), which is what the amp's block bytes hold.

use anyhow::{bail, Context, Result};
use roxmltree::Document;

use crate::models;
use crate::protocol::{
    self, Kind, Message, Preset, AMP_TO_HOST, FRAME_MARKER, OFF_MODEL, OFF_PARAMS, OFF_SLOT,
    REPORT_SIZE,
};

/// Parse a `.fuse` XML document into a preset in the live buffer.
///
/// Importing into the live buffer matches what FUSE does when you open a
/// preset: it becomes the sound you are editing, not a stored slot.
pub fn parse(text: &str) -> Result<Preset> {
    let doc = Document::parse(text).context("parsing .fuse XML")?;
    let root = doc.root_element();
    if root.tag_name().name() != "Preset" {
        bail!(
            "not a FUSE preset: root element is <{}>",
            root.tag_name().name()
        );
    }

    let name = root.attribute("name").unwrap_or("Imported preset");
    let mut preset = Preset::new(protocol::BANK_LIVE, 0);
    preset.set_name(name);

    for module in root
        .descendants()
        .filter(|node| node.has_tag_name("Module"))
    {
        let Some(kind) = kind_of(&module) else {
            continue;
        };
        if let Some(message) = module_message(&module, kind)? {
            preset.add(message);
        }
    }

    if preset.block(Kind::Amp).is_none() {
        bail!("the .fuse file has no <Amplifier><Module> block");
    }
    Ok(preset)
}

/// Write one preset as a `.fuse` document.
pub fn export(preset: &Preset) -> Result<String> {
    let name = if preset.name.is_empty() {
        "Custom Preset"
    } else {
        &preset.name
    };
    let escape = escape_xml(name);

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str(&format!(
        "<Preset name=\"{escape}\" amplifier=\"Mustang\" productId=\"1\">\n"
    ));
    out.push_str("  <Amplifier>\n");
    if let Some(amp) = preset.block(Kind::Amp) {
        let model = amp.model().unwrap_or(0);
        let id = models::fuse_id(Kind::Amp, model).unwrap_or(model as u32);
        out.push_str(&format!(
            "    <Module ID=\"{id}\" POS=\"0\" BypassState=\"0\">\n"
        ));
        for index in 0..8 {
            let value = u32::from(amp.byte(OFF_PARAMS + index)) * 257;
            out.push_str(&format!(
                "      <Param ControlIndex=\"{index}\">{value}</Param>\n"
            ));
        }
        out.push_str("    </Module>\n");
    }
    out.push_str("  </Amplifier>\n");
    out.push_str("  <FX>\n");
    for (tag, kind) in [
        ("Stompbox", Kind::Stomp),
        ("Modulation", Kind::Mod),
        ("Delay", Kind::Delay),
        ("Reverb", Kind::Reverb),
    ] {
        match preset
            .block(kind)
            .filter(|block| block.model().unwrap_or(0) != 0)
        {
            Some(block) => {
                out.push_str(&format!("    <{tag}>\n"));
                out.push_str(&format!(
                    "      <Module ID=\"{}\" POS=\"{}\" BypassState=\"{}\">\n",
                    models::fuse_id(kind, block.model().unwrap_or(0))
                        .unwrap_or(block.model().unwrap_or(0) as u32),
                    block.slot().unwrap_or(0),
                    u8::from(block.byte(22) != 0),
                ));
                let labels = models::effect_params(kind, block.model().unwrap_or(0));
                let count = if labels.is_empty() { 6 } else { labels.len() };
                for index in 0..count {
                    if labels.get(index).is_some_and(|label| label.is_empty()) {
                        continue;
                    }
                    let value = u32::from(block.byte(OFF_PARAMS + index)) * 257;
                    out.push_str(&format!(
                        "        <Param ControlIndex=\"{index}\">{value}</Param>\n"
                    ));
                }
                out.push_str("      </Module>\n");
                out.push_str(&format!("    </{tag}>\n"));
            }
            None => out.push_str(&format!("    <{tag} />\n")),
        }
    }
    out.push_str("  </FX>\n");
    out.push_str(&format!("  <Info name=\"{escape}\"></Info>\n</Preset>\n"));
    Ok(out)
}

fn kind_of(module: &roxmltree::Node<'_, '_>) -> Option<Kind> {
    module
        .ancestors()
        .filter(|node| node.is_element())
        .find_map(|node| match node.tag_name().name() {
            "Amplifier" => Some(Kind::Amp),
            "Stompbox" => Some(Kind::Stomp),
            "Modulation" => Some(Kind::Mod),
            "Delay" => Some(Kind::Delay),
            "Reverb" => Some(Kind::Reverb),
            _ => None,
        })
}

fn module_message(module: &roxmltree::Node<'_, '_>, kind: Kind) -> Result<Option<Message>> {
    let raw_id = module
        .attribute("ID")
        .unwrap_or("0")
        .trim()
        .parse::<u32>()
        .with_context(|| format!("bad Module ID {:?}", module.attribute("ID")))?;
    let Some(model) = models::from_fuse_id(kind, raw_id) else {
        // Unknown to our table: keep the file usable instead of dropping the
        // whole preset. FUSE ids are small, so an unknown id is almost always
        // a model this build does not know yet.
        return Ok(None);
    };
    if kind != Kind::Amp && model == 0 {
        return Ok(None);
    }

    let slot = module
        .attribute("POS")
        .and_then(|value| value.trim().parse::<u8>().ok())
        .unwrap_or(0);
    let bypass = module
        .attribute("BypassState")
        .is_some_and(|value| value.trim() == "1");

    let mut raw = [0u8; REPORT_SIZE];
    raw[0] = FRAME_MARKER;
    raw[1] = AMP_TO_HOST;
    raw[2] = kind.code();
    raw[3] = protocol::BANK_LIVE;
    raw[4] = 0;
    raw[6] = 0x01;
    raw[7] = 0x01;
    let [low, high] = model.to_le_bytes();
    raw[OFF_MODEL] = low;
    raw[OFF_MODEL + 1] = high;
    if kind.has_slot() {
        raw[OFF_SLOT] = slot;
    }
    raw[22] = u8::from(bypass);
    if kind == Kind::Amp {
        if let Some(voice) = models::voice_index(model) {
            raw[44..=46].fill(voice);
        }
    }

    for param in module.children().filter(|node| node.has_tag_name("Param")) {
        let index = param
            .attribute("ControlIndex")
            .unwrap_or("0")
            .trim()
            .parse::<usize>()
            .unwrap_or(usize::MAX);
        if index >= REPORT_SIZE - OFF_PARAMS {
            continue;
        }
        let value = param
            .text()
            .unwrap_or("0")
            .trim()
            .parse::<u32>()
            .unwrap_or(0);
        // FUSE writes `byte * 257`; accept a plain 0..255 value as well.
        let byte = if value <= 0xff {
            value as u8
        } else {
            (value >> 8) as u8
        };
        raw[OFF_PARAMS + index] = byte;
    }

    Ok(Message::from_bytes(raw))
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
