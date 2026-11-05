use std::path::PathBuf;

use scx2::capture;
use scx2::models;
use scx2::protocol::{self, Dump, Kind, BANK_AMP, BANK_FX, BANK_LIVE};

fn capture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("captures")
        .join("fuse-scx2-2021-06-07.pcap")
}

fn loaded_dump() -> Dump {
    let reports = capture::amp_reports(&capture_path()).expect("read capture");
    Dump::from_reports(reports)
}

#[test]
fn recall_frames_carry_a_bank_and_slot() {
    let amp = protocol::patch_change(7);
    assert_eq!((amp[0], amp[1], amp[2]), (0x1c, 0x01, 0x01));
    assert_eq!((amp[3], amp[4]), (0, 7));

    let fx = protocol::recall_preset(BANK_FX, 3);
    assert_eq!((fx[3], fx[4]), (BANK_FX, 3));
}

#[test]
fn a_name_block_is_nul_padded_and_retargetable() {
    let raw = protocol::name_change(BANK_AMP, 4, "Tweed");
    assert_eq!(raw[2], Kind::Name.code());
    assert_eq!((raw[3], raw[4]), (BANK_AMP, 4));
    assert_eq!(&raw[16..21], b"Tweed");
    assert!(raw[21..48].iter().all(|byte| *byte == 0));

    let mut block = [0u8; protocol::REPORT_SIZE];
    block[0] = 0x1c;
    block[1] = 0x01;
    block[2] = Kind::Stomp.code();
    block[16] = 0xba;
    block[32] = 0x42;
    let moved = protocol::retarget(&block, BANK_AMP, 9);
    assert_eq!((moved[3], moved[4]), (BANK_AMP, 9));
    assert_eq!(moved[16], 0xba);
    assert_eq!(moved[32], 0x42);
}

#[test]
fn the_save_frame_carries_a_bank_slot_and_name() {
    let frame = protocol::save_preset(BANK_FX, 15, "Saved");
    assert_eq!((frame[0], frame[1], frame[2]), (0x1c, 0x01, 0x03));
    assert_eq!((frame[3], frame[4]), (BANK_FX, 15));
    assert_eq!((frame[6], frame[7]), (0x01, 0x01));
    assert_eq!(&frame[16..21], b"Saved");
    assert!(frame[21..48].iter().all(|byte| *byte == 0));

    let amp = protocol::save_preset(BANK_AMP, 7, "Amp");
    assert_eq!((amp[3], amp[4]), (BANK_AMP, 7));
}

#[test]
fn the_bypass_frame_uses_kind_minus_three() {
    let reverb = protocol::bypass(Kind::Reverb, 5, true);
    assert_eq!(
        (reverb[0], reverb[1], reverb[2], reverb[3], reverb[4]),
        (0x19, protocol::HOST_CONTROL, 6, 1, 5)
    );
    let stomp = protocol::bypass(Kind::Stomp, 3, false);
    assert_eq!((stomp[2], stomp[3], stomp[4]), (3, 0, 3));
    let delay = protocol::bypass(Kind::Delay, 1, true);
    assert_eq!(delay[2], 5);
    let modulation = protocol::bypass(Kind::Mod, 4, false);
    assert_eq!(modulation[2], 4);
}

#[test]
fn capture_is_a_complete_dump() {
    let dump = loaded_dump();
    assert!(dump.ended, "the capture ends with the amp acknowledgement");
    assert_eq!(dump.presets().count(), 34);
    // Handshake banners plus the live notification reports.
    assert_eq!(dump.unparsed.len(), 7);
}

#[test]
fn handshake_banners_carry_the_firmware_build_date() {
    let reports = capture::amp_reports(&capture_path()).unwrap();
    let banners: Vec<String> = reports
        .iter()
        .filter_map(|r| protocol::boot_banner(r))
        .collect();
    assert_eq!(banners, vec!["May 30 2013".to_string(); 2]);
}

#[test]
fn banks_have_the_expected_shape() {
    let dump = loaded_dump();
    assert_eq!(dump.bank(BANK_LIVE).count(), 1);
    assert_eq!(dump.bank(BANK_AMP).count(), 17);
    assert_eq!(dump.bank(BANK_FX).count(), 16);
}

#[test]
fn amp_presets_decode_to_known_models_and_names() {
    let dump = loaded_dump();

    let clean = dump.get(BANK_AMP, 1).expect("amp preset 1");
    assert_eq!(clean.name, "Tweed Deluxe Clean");
    assert_eq!(clean.amp_model(), Some(0x0067));
    assert_eq!(
        models::model_name(Kind::Amp, clean.amp_model().unwrap()),
        Some("'57 Deluxe")
    );

    let twin = dump.get(BANK_AMP, 7).expect("amp preset 7");
    assert_eq!(twin.name, "'65 Twin");
    assert_eq!(twin.amp_model(), Some(0x0075));

    let clean_channel = dump.get(BANK_AMP, 16).expect("amp preset 16");
    assert_eq!(clean_channel.name, "Clean Channel");
    assert_eq!(clean_channel.amp_model(), Some(0x00eb));
}

#[test]
fn stomp_block_is_present_only_where_a_pedal_is_loaded() {
    let dump = loaded_dump();

    let clean = dump.get(BANK_AMP, 1).unwrap();
    let dirty = dump.get(BANK_AMP, 2).unwrap();

    assert_eq!(clean.block(Kind::Stomp).unwrap().model(), Some(0x0000));
    assert_eq!(dirty.block(Kind::Stomp).unwrap().model(), Some(0x0007));
    assert_eq!(models::model_name(Kind::Stomp, 0x0007), Some("Compressor"));
}

#[test]
fn fx_presets_carry_mod_delay_and_reverb() {
    let dump = loaded_dump();

    let tremolo = dump.get(BANK_FX, 0).expect("fx preset 0");
    assert_eq!(tremolo.name, "Slow Vibratone");
    assert_eq!(
        tremolo.block(Kind::Mod).unwrap().model(),
        Some(0x002d),
        "Vibratone"
    );

    let spring = dump.get(BANK_FX, 6).expect("fx preset 6");
    assert_eq!(spring.name, "Fender Spring Reverb");
    assert_eq!(
        spring.block(Kind::Reverb).unwrap().model(),
        Some(0x000b),
        "Spring '65"
    );
}

#[test]
fn live_buffer_holds_the_current_patch() {
    let dump = loaded_dump();
    let live = dump.live().expect("live buffer");
    assert_eq!(live.bank, BANK_LIVE);
    assert_eq!(live.amp_model(), Some(0x00eb));
    assert!(live.block(Kind::Delay).is_some());
    assert!(live.block(Kind::Reverb).is_some());
}

#[test]
fn reports_round_trip_through_json() {
    let dump = loaded_dump();
    let json = dump.to_json();
    let presets = json["presets"].as_array().expect("presets array");
    assert_eq!(presets.len(), 34);
    assert_eq!(json["ended"], true);
}

#[test]
fn host_frames_are_64_bytes() {
    assert_eq!(protocol::request_dump().len(), protocol::REPORT_SIZE);
    assert_eq!(protocol::handshake_step1()[..2], [0x00, 0xC3]);
    assert_eq!(protocol::handshake_step2()[..2], [0x1A, 0xC1]);
    assert_eq!(
        protocol::patch_change(3)[..5],
        [0x1C, 0x01, 0x01, 0x00, 0x03]
    );
}

#[test]
fn backup_round_trips_losslessly() {
    let dump = loaded_dump();
    let text = serde_json::to_string(&dump.to_json()).unwrap();
    let restored = Dump::from_json(&text).unwrap();
    assert_eq!(restored.presets().count(), dump.presets().count());
    assert_eq!(
        restored.to_json()["presets"],
        dump.to_json()["presets"],
        "every raw report survives the round trip"
    );
    assert_eq!(restored.live().unwrap().amp_model(), Some(0x00eb));
}

#[test]
fn diff_finds_a_single_changed_parameter_byte() {
    let dump = loaded_dump();
    let live = dump.live().unwrap();
    let mut patched = live.clone();
    patched
        .blocks
        .get_mut(&Kind::Amp.code())
        .unwrap()
        .set_byte(35, 0x63);

    let changes = protocol::diff_presets(live, &patched);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].offset, 35);
    assert_eq!(changes[0].before, live.block(Kind::Amp).unwrap().byte(35));
    assert_eq!(changes[0].after, 0x63);
}

#[test]
fn identical_dumps_have_no_diff() {
    let dump = loaded_dump();
    assert!(protocol::diff_dumps(&dump, &dump).is_empty());
}

#[test]
fn hex_parsing_accepts_spaces_and_direction_is_the_only_change() {
    let bytes = protocol::parse_hex("1c 01 05 ff").unwrap();
    assert_eq!(bytes, vec![0x1c, 0x01, 0x05, 0xff]);
    assert_eq!(protocol::parse_hex("1c0105ff").unwrap(), bytes);
    assert!(protocol::parse_hex("1c0").is_none());

    let mut raw = [0u8; protocol::REPORT_SIZE];
    raw[0] = 0x1c;
    raw[1] = 0x01;
    raw[35] = 0x80;
    let host = protocol::as_host_frame(&raw, protocol::HOST_CONTROL);
    assert_eq!(host[1], 0xc3);
    assert_eq!(host[0], 0x1c);
    assert_eq!(host[35], 0x80);
}
