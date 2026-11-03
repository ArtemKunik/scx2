use super::*;

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
