//! The knob catalogue: what the editor offers and how a knob maps to the amp.

use scx2::params::{self, KNOBS};

#[test]
fn standard_knobs_match_fuse() {
    let standard: Vec<_> = KNOBS.iter().filter(|spec| spec.standard).collect();
    assert_eq!(
        standard.iter().map(|spec| spec.name).collect::<Vec<_>>(),
        ["Volume", "Gain", "Treble", "Middle", "Bass", "Voice"]
    );
    assert_eq!(
        standard.iter().map(|spec| spec.offset).collect::<Vec<_>>(),
        [32, 33, 36, 37, 38, 44]
    );
}

#[test]
fn hidden_knobs_have_names_and_honest_hints() {
    let hidden: Vec<_> = KNOBS.iter().filter(|spec| !spec.standard).collect();
    assert!(
        hidden.len() >= 8,
        "expected the probed extras to be offered"
    );
    for spec in hidden {
        assert!(!spec.name.is_empty());
        assert!(!spec.hint.is_empty(), "{} needs an explanation", spec.name);
    }

    // These still have no authoritative SCX2 description, so their hints must
    // say so. Resonance and Gate depth are named by FUSE's own model table and
    // no longer count as guesses.
    for name in [
        "Gain 2",
        "Master",
        "Presence",
        "Bias",
        "Gate threshold",
        "Sag",
        "Bright",
    ] {
        let spec = KNOBS
            .iter()
            .find(|spec| spec.name == name)
            .unwrap_or_else(|| panic!("{name} is missing from the knob catalogue"));
        assert!(
            spec.hint.to_ascii_lowercase().contains("guess"),
            "{name} needs a hint that says it is a guess"
        );
    }
}

#[test]
fn the_cabinet_is_not_a_knob() {
    // The speaker is chosen in the combined amp/speaker picker, next to the
    // voice. It must not appear as another knob on the panel.
    assert!(
        KNOBS.iter().all(|spec| spec.offset != 49),
        "the cabinet must stay out of the knob catalogue"
    );
    assert_eq!(params::index_for_offset(49), Some(0x11));
}

#[test]
fn every_knob_is_unique_and_has_a_label() {
    let mut offsets: Vec<usize> = KNOBS.iter().map(|spec| spec.offset).collect();
    offsets.sort_unstable();
    offsets.dedup();
    assert_eq!(offsets.len(), KNOBS.len(), "duplicate knob offset");

    for spec in KNOBS {
        assert!(
            params::offset_label(spec.offset).is_some(),
            "{} has no parameter-grid label",
            spec.name
        );
        assert_eq!(
            params::knob_spec(spec.offset).map(|found| found.name),
            Some(spec.name)
        );
    }
}

#[test]
fn amp_parameters_round_trip_through_magic() {
    for value in 0u8..=255 {
        let magic = params::magic_from_byte(value);
        assert_eq!((magic as f32 / 257.0).round() as u8, value);
    }
}

#[test]
fn hidden_amp_knobs_can_be_written_by_direct_control() {
    for offset in [32usize, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 49, 51, 52] {
        assert!(
            params::index_for_offset(offset).is_some(),
            "offset {offset} has no direct-control index"
        );
    }
    // Voice and the gate pair use block writes, not direct control.
    for offset in [44usize, 47, 48] {
        assert!(params::index_for_offset(offset).is_none());
    }
}
