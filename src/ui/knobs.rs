//! Which knobs are on the panel: adding, removing and the limit.

use crate::params;

use super::app::Knob;

/// Knobs are capped so the panel stays readable; every one can still be
/// removed again.
pub(crate) const MAX_KNOBS: usize = 16;

/// Bind a block offset to a knob. Returns false if it is already bound.
pub(crate) fn add_knob(knobs: &mut Vec<Knob>, offset: usize) -> bool {
    if knobs.len() >= MAX_KNOBS || knobs.iter().any(|knob| knob.offset == offset) {
        return false;
    }
    knobs.push(Knob {
        offset,
        label: params::offset_title(offset).unwrap_or_else(|| format!("P{offset}")),
    });
    true
}

/// Unbind the knob for an offset. Returns whether one was removed.
pub(crate) fn remove_knob(knobs: &mut Vec<Knob>, offset: usize) -> bool {
    let before = knobs.len();
    knobs.retain(|knob| knob.offset != offset);
    knobs.len() != before
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol;

    #[test]
    fn a_knob_can_be_added_and_removed_again() {
        let mut knobs = Vec::new();
        assert!(add_knob(&mut knobs, 32));
        assert!(add_knob(&mut knobs, 39));
        assert_eq!(knobs.len(), 2);
        assert_eq!(knobs[0].label, "Volume");

        assert!(!add_knob(&mut knobs, 39), "duplicates are rejected");
        assert!(remove_knob(&mut knobs, 39));
        assert_eq!(knobs.len(), 1);
        assert_eq!(knobs[0].offset, 32);
        assert!(!remove_knob(&mut knobs, 39), "removing twice is harmless");
    }

    #[test]
    fn the_knob_limit_is_enforced() {
        let mut knobs = Vec::new();
        for offset in protocol::OFF_PARAMS..protocol::OFF_PARAMS + MAX_KNOBS {
            assert!(add_knob(&mut knobs, offset));
        }
        assert!(!add_knob(&mut knobs, protocol::REPORT_SIZE - 1));
        assert_eq!(knobs.len(), MAX_KNOBS);
        assert!(remove_knob(&mut knobs, protocol::OFF_PARAMS));
        assert!(add_knob(&mut knobs, protocol::REPORT_SIZE - 1));
    }
}
