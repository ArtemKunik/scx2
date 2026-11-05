//! The effect slots: choosing a model and editing its parameter bytes.

use std::path::PathBuf;

use scx2::capture;
use scx2::models;
use scx2::protocol::{self, Dump, Kind, Preset, OFF_MODEL, OFF_PARAMS};

const EFFECT_KINDS: [Kind; 4] = [Kind::Stomp, Kind::Mod, Kind::Delay, Kind::Reverb];

#[test]
fn every_effect_kind_has_a_model_table_starting_with_none() {
    for kind in EFFECT_KINDS {
        let table = models::table(kind).unwrap_or_else(|| panic!("{kind:?} has no model table"));
        assert_eq!(table[0], (0, "None"), "{kind:?} must start with None");

        let mut ids = Vec::new();
        for (id, name) in table {
            assert!(!name.is_empty(), "{kind:?} has an unnamed model");
            assert!(!ids.contains(id), "{kind:?} lists model 0x{id:04x} twice");
            ids.push(*id);
            assert_eq!(models::model_name(kind, *id), Some(*name));
        }
    }
}

#[test]
fn choosing_an_effect_writes_the_little_endian_model_id() {
    let mut preset = Preset::new(0, 0);
    assert!(preset.set_model(Kind::Mod, 0x0012));

    let block = preset.block(Kind::Mod).expect("created block");
    assert_eq!(block.model(), Some(0x0012));
    assert_eq!(block.byte(OFF_MODEL), 0x12);
    assert_eq!(block.byte(OFF_MODEL + 1), 0x00);

    // Choosing another model reuses the block and keeps its parameters.
    preset
        .block_mut(Kind::Mod)
        .unwrap()
        .set_byte(OFF_PARAMS, 0x7f);
    assert!(preset.set_model(Kind::Mod, 0x002d));
    let block = preset.block(Kind::Mod).unwrap();
    assert_eq!(block.model(), Some(0x002d));
    assert_eq!(block.byte(OFF_PARAMS), 0x7f);
}

#[test]
fn clearing_an_effect_sets_the_model_to_none() {
    let mut preset = Preset::new(0, 0);
    preset.set_model(Kind::Delay, 0x002b);
    assert!(preset.set_model(Kind::Delay, 0));
    assert_eq!(preset.block(Kind::Delay).unwrap().model(), Some(0));
}

#[test]
fn a_missing_effect_block_is_created_for_the_right_preset() {
    let mut preset = Preset::new(2, 7);
    assert!(preset.block(Kind::Reverb).is_none());
    assert!(preset.set_model(Kind::Reverb, 0x0024));

    let block = preset.block(Kind::Reverb).expect("created block");
    assert_eq!(block.kind(), Kind::Reverb);
    assert_eq!(block.bank(), 2);
    assert_eq!(block.index(), 7);
    assert_eq!(block.model(), Some(0x0024));
    assert!(
        block.byte(18) <= 6,
        "the slot byte stays in the observed range"
    );
}

#[test]
fn kinds_without_a_model_are_rejected() {
    let mut preset = Preset::new(0, 0);
    assert!(!preset.set_model(Kind::Name, 1));
    assert!(!preset.set_model(Kind::State, 1));
    assert!(preset.block(Kind::Name).is_none());
    assert!(preset.block(Kind::State).is_none());
}

#[test]
fn loading_an_effect_is_a_model_id_diff() {
    let mut before = Preset::new(0, 0);
    before.set_model(Kind::Mod, 0);
    let mut after = before.clone();
    // A model id with a non-zero high byte proves both bytes travel.
    after.set_model(Kind::Mod, 0x101f);

    let changes = protocol::diff_presets(&before, &after);
    assert!(
        changes
            .iter()
            .any(|change| change.offset == OFF_MODEL && change.after == 0x1f),
        "the low model byte must be sent"
    );
    assert!(
        changes
            .iter()
            .any(|change| change.offset == OFF_MODEL + 1 && change.after == 0x10),
        "the high model byte must be sent"
    );
    assert!(changes.iter().all(|change| change.kind == Kind::Mod));
}

#[test]
fn a_knob_edit_becomes_a_single_parameter_change() {
    let mut before = Preset::new(0, 0);
    before.set_model(Kind::Mod, 0x0012);
    let mut after = before.clone();
    after
        .block_mut(Kind::Mod)
        .unwrap()
        .set_byte(OFF_PARAMS + 1, 0x40);

    let changes = protocol::diff_presets(&before, &after);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].offset, OFF_PARAMS + 1);
    assert_eq!(changes[0].after, 0x40);
    assert_eq!(changes[0].kind, Kind::Mod);
}

#[test]
fn every_effect_model_has_real_parameter_names() {
    for kind in EFFECT_KINDS {
        for (model, name) in models::table(kind)
            .unwrap()
            .iter()
            .filter(|(id, _)| *id != 0)
        {
            let labels = models::effect_params(kind, *model);
            assert!(
                !labels.is_empty(),
                "{kind:?} model {name} has no parameter names"
            );
            assert!(
                !labels[0].is_empty(),
                "{kind:?} model {name} starts with an empty parameter"
            );
            let knobs = models::effect_knobs(kind, *model);
            assert!(
                (1..=6).contains(&knobs),
                "{kind:?} model {name} claims {knobs} knobs"
            );
            assert!(
                labels[..knobs].iter().all(|label| !label.is_empty()),
                "{kind:?} model {name} has a gap in its knob list"
            );
        }
    }
}

#[test]
fn loading_a_model_replaces_the_old_parameters_with_its_defaults() {
    let mut preset = Preset::new(0, 0);
    preset.set_model(Kind::Stomp, 0x003c);
    preset
        .block_mut(Kind::Stomp)
        .unwrap()
        .set_byte(OFF_PARAMS, 0x01);
    assert!(preset.load_model(Kind::Stomp, 0x0007));

    let block = preset.block(Kind::Stomp).unwrap();
    assert_eq!(block.model(), Some(0x0007));
    assert_eq!(
        &block.params()[..5],
        &[0x8d, 0x0f, 0x4f, 0x7f, 0x7f],
        "the compressor's own defaults, not the overdrive's byte"
    );

    // A model without a captured default still gets a neutral value in every
    // slot it uses instead of inheriting the previous model's bytes.
    assert!(preset.load_model(Kind::Stomp, 0x001c));
    let block = preset.block(Kind::Stomp).unwrap();
    assert_eq!(&block.params()[..5], &[0x80; 5]);
}

#[test]
fn clearing_a_model_zeroes_its_parameters() {
    let mut preset = Preset::new(0, 0);
    preset.load_model(Kind::Delay, 0x002b);
    assert!(preset.load_model(Kind::Delay, 0));
    let block = preset.block(Kind::Delay).unwrap();
    assert_eq!(block.model(), Some(0));
    assert!(block.params().iter().all(|byte| *byte == 0));
}

#[test]
fn fuse_ids_round_trip_for_every_known_model() {
    for kind in [Kind::Amp, Kind::Stomp, Kind::Mod, Kind::Delay, Kind::Reverb] {
        for (model, name) in models::table(kind).unwrap() {
            if let Some(id) = models::fuse_id(kind, *model) {
                assert_eq!(
                    models::from_fuse_id(kind, id),
                    Some(*model),
                    "{kind:?} model {name} lost its FUSE id {id}"
                );
            }
        }
    }
    assert_eq!(models::from_fuse_id(Kind::Amp, 105), Some(0x0053));
    assert_eq!(models::from_fuse_id(Kind::Amp, 108), Some(0x0072));
}

#[test]
fn the_observed_model_list_matches_the_bundled_capture() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("captures")
        .join("fuse-scx2-2021-06-07.pcap");
    let reports = capture::amp_reports(&path).expect("read capture");
    let dump = Dump::from_reports(reports);

    let mut seen = Vec::new();
    for preset in dump.presets() {
        for kind in EFFECT_KINDS {
            if let Some(model) = preset.block(kind).and_then(|block| block.model()) {
                if model != 0 && !seen.contains(&(kind, model)) {
                    seen.push((kind, model));
                }
            }
        }
    }
    let mut expected = models::OBSERVED.to_vec();
    seen.sort();
    expected.sort();
    assert_eq!(seen, expected, "update models::OBSERVED");
}

#[test]
fn effect_parameter_bytes_are_independent_of_the_model() {
    let mut preset = Preset::new(0, 0);
    preset.set_model(Kind::Stomp, 0x00ba);
    let block = preset.block_mut(Kind::Stomp).unwrap();
    block.set_byte(OFF_PARAMS, 0x40);
    block.set_byte(OFF_PARAMS + 1, 0x80);
    block.set_byte(OFF_PARAMS + 2, 0xc0);

    let block = preset.block(Kind::Stomp).unwrap();
    assert_eq!(block.model(), Some(0x00ba));
    assert_eq!(
        &block.params()[..3],
        &[0x40, 0x80, 0xc0],
        "the knobs edit bytes 32, 33 and 34"
    );
}
