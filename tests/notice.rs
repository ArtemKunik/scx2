//! Reports the amp sends by itself. The bytes are from a real session with a
//! Super Champ X2 (fw `May 30 2013`) while its front-panel knobs were turned.

use scx2::params;
use scx2::protocol::{classify, Notice, REPORT_SIZE};

fn report(head: &[u8]) -> [u8; REPORT_SIZE] {
    let mut bytes = [0u8; REPORT_SIZE];
    bytes[..head.len()].copy_from_slice(head);
    bytes
}

fn knob(index: u8, value: u8) -> [u8; REPORT_SIZE] {
    report(&[
        0x05, 0x01, 0x02, 0x53, 0x00, index, index, 0x0c, 0x00, value, value,
    ])
}

#[test]
fn turning_volume_reports_its_new_value() {
    assert_eq!(
        classify(&knob(0x00, 0x2d)),
        Some(Notice::AmpParam {
            model: 0x0053,
            index: 0,
            value: 0x2d,
        })
    );
}

#[test]
fn turning_gain_reports_its_new_value() {
    assert_eq!(
        classify(&knob(0x01, 0xfe)),
        Some(Notice::AmpParam {
            model: 0x0053,
            index: 1,
            value: 0xfe,
        })
    );
}

#[test]
fn a_reported_value_matches_what_a_write_would_send() {
    for value in 0..=u8::MAX {
        let wire = params::magic_from_byte(value).to_le_bytes();
        let frame = report(&[
            0x05, 0x01, 0x02, 0x53, 0x00, 0x05, 0x05, 0x0c, 0x00, wire[0], wire[1],
        ]);
        match classify(&frame) {
            Some(Notice::AmpParam { value: read, .. }) => assert_eq!(read, value),
            other => panic!("{value:#04x} classified as {other:?}"),
        }
    }
}

#[test]
fn a_write_is_acknowledged_with_a_status() {
    let accepted = report(&[0x00, 0x00, 0x05, 0x00, b'M', b'a', b'y']);
    assert_eq!(classify(&accepted), Some(Notice::Ack { status: 0 }));
    let rejected = report(&[0x00, 0x00, 0x05, 0x03]);
    assert_eq!(classify(&rejected), Some(Notice::Ack { status: 3 }));
}

#[test]
fn the_handshake_is_not_an_acknowledgement() {
    let banner = report(&[0x00, 0x00, 0x00, 0x00, b'M', b'a', b'y']);
    assert_eq!(classify(&banner), None);
}

#[test]
fn selector_and_effect_reports_ask_for_a_new_read() {
    // Seen at the end of every dump: selector knob positions and an effect value.
    for head in [
        &[0x1d, 0x01, 0x01, 0x06][..],
        &[0x1d, 0x01, 0x02, 0x05][..],
        &[
            0x05, 0x01, 0x06, 0x3a, 0x00, 0x00, 0x00, 0x01, 0x00, 0x1d, 0x1d,
        ][..],
    ] {
        assert_eq!(classify(&report(head)), Some(Notice::StateChanged));
    }
}

#[test]
fn block_and_end_of_dump_reports_are_not_notices() {
    assert_eq!(classify(&report(&[0x1c, 0x01, 0x05, 0x00])), None);
    assert_eq!(classify(&report(&[0xff, 0x01])), None);
}

#[test]
fn every_direct_control_offset_maps_back_to_itself() {
    for offset in 32..64 {
        if let Some(index) = params::index_for_offset(offset) {
            assert_eq!(params::offset_for_index(index), Some(offset));
        }
    }
}
