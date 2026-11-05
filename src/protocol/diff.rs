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

/// The first byte that carries preset data instead of frame routing.
///
/// Bytes 0..8 are the marker, direction, kind, bank, index, padding and the
/// kind-dependent flag. The amp flips byte 7 to `0x01` after an execute frame,
/// and a stored copy moves bank/index, so a read-back check must compare the
/// payload and not the header.
pub const PAYLOAD_START: usize = 8;

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

/// Payload differences between two versions of one preset, header ignored.
///
/// This is the read-back check for a store: the amp copies its live buffer
/// into a slot, so everything from the model id down should match once the
/// bank/index and the execute flag are left out.
pub fn diff_payload(a: &Preset, b: &Preset) -> Vec<Change> {
    diff_presets(a, b)
        .into_iter()
        .filter(|change| change.offset >= PAYLOAD_START)
        .collect()
}

/// Blocks that carry a loaded model in `full` but are absent from `partial`.
///
/// A cleared effect can be reported by the live buffer and left out of a
/// stored preset; that is not data loss. A loaded model that vanishes is, so
/// it is reported separately from the byte diff.
pub fn models_missing_from(full: &Preset, partial: &Preset) -> Vec<(Kind, u16)> {
    full.blocks
        .iter()
        .filter_map(|(code, message)| {
            let model = message.model().filter(|model| *model != 0)?;
            (!partial.blocks.contains_key(code)).then(|| (Kind::from_code(*code), model))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A preset built from raw reports, so the tests see the same bytes the
    /// amp sends.
    fn preset(bank: u8, index: u8, volume: u8) -> Preset {
        let mut amp = [0u8; REPORT_SIZE];
        amp[0] = FRAME_MARKER;
        amp[1] = AMP_TO_HOST;
        amp[2] = Kind::Amp.code();
        amp[3] = bank;
        amp[4] = index;
        amp[6] = 0x01;
        amp[OFF_MODEL] = 0x21;
        amp[OFF_MODEL + 1] = 0x00;
        amp[32] = volume;
        let mut preset = Preset::new(bank, index);
        preset.add(Message::from_bytes(amp).expect("a valid amp report"));
        preset
    }

    #[test]
    fn a_payload_diff_ignores_the_frame_header() {
        let live = preset(BANK_LIVE, 0, 0x63);
        let stored = preset(BANK_AMP, 7, 0x63);
        assert!(
            !diff_presets(&live, &stored).is_empty(),
            "the header does move between a live buffer and a stored slot"
        );
        assert!(
            diff_payload(&live, &stored).is_empty(),
            "the same payload in another slot is a match"
        );
    }

    #[test]
    fn a_payload_diff_finds_a_changed_parameter() {
        let live = preset(BANK_LIVE, 0, 0x63);
        let stored = preset(BANK_AMP, 7, 0x40);
        let changes = diff_payload(&live, &stored);
        assert_eq!(changes.len(), 1);
        assert_eq!((changes[0].kind, changes[0].offset), (Kind::Amp, 32));
    }

    #[test]
    fn a_loaded_model_missing_from_the_stored_copy_is_reported() {
        let live = preset(BANK_LIVE, 0, 0x63);
        let stored = Preset::new(BANK_AMP, 7);
        let missing = models_missing_from(&live, &stored);
        assert_eq!(missing, vec![(Kind::Amp, 0x21)]);
        assert_eq!(models_missing_from(&stored, &live), vec![]);
    }
}
