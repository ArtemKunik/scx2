//! Reader for USBPcap captures (`.pcap`, link type 249).
//!
//! Handy for offline work: a capture made with USBPcap while Fender FUSE was
//! talking to the amp replays through exactly the same decoder the hardware
//! path uses. `captures/` ships one such recording.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::protocol::REPORT_SIZE;

/// USBPcap's libpcap link type.
pub const LINKTYPE_USBPCAP: u32 = 249;

const MAGIC_US: u32 = 0xA1B2_C3D4;
const MAGIC_NS: u32 = 0xA1B2_3C4D;

/// USBPcap transfer type for interrupt endpoints (2 is control).
const TRANSFER_INTERRUPT: u8 = 1;

#[derive(Debug, Clone)]
pub struct Packet {
    pub bus: u16,
    pub device: u16,
    pub endpoint: u8,
    pub transfer: u8,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn is_in(&self) -> bool {
        self.endpoint & 0x80 != 0
    }

    pub fn is_interrupt(&self) -> bool {
        self.transfer == TRANSFER_INTERRUPT
    }
}

struct Reader<'a> {
    data: &'a [u8],
    big_endian: bool,
}

impl<'a> Reader<'a> {
    fn u32(&self, off: usize) -> u32 {
        let bytes = [
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
        ];
        if self.big_endian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }
    }
}

/// Parse every packet in a USBPcap file.
pub fn read_packets(path: &Path) -> Result<Vec<Packet>> {
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    packets_from_bytes(&data).with_context(|| format!("parsing {}", path.display()))
}

/// Parse every packet from an in-memory USBPcap file.
pub fn packets_from_bytes(data: &[u8]) -> Result<Vec<Packet>> {
    if data.len() < 24 {
        bail!("too short to be a pcap file");
    }

    let magic_le = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let big_endian = match magic_le {
        MAGIC_US | MAGIC_NS => false,
        m if m.swap_bytes() == MAGIC_US || m.swap_bytes() == MAGIC_NS => true,
        other => bail!("unknown pcap magic 0x{other:08x}"),
    };

    let reader = Reader { data, big_endian };
    let linktype = reader.u32(20);
    if linktype != LINKTYPE_USBPCAP {
        bail!("link type {linktype}, expected {LINKTYPE_USBPCAP} (USBPcap)");
    }

    let mut packets = Vec::new();
    let mut off = 24usize;
    while off + 16 <= data.len() {
        let incl_len = reader.u32(off + 8) as usize;
        off += 16;
        if off + incl_len > data.len() {
            break;
        }
        let frame = &data[off..off + incl_len];
        off += incl_len;
        if frame.len() < 27 {
            continue;
        }

        let header_len = u16::from_le_bytes([frame[0], frame[1]]) as usize;
        let bus = u16::from_le_bytes([frame[17], frame[18]]);
        let device = u16::from_le_bytes([frame[19], frame[20]]);
        let endpoint = frame[21];
        let transfer = frame[22];
        let data_len = u32::from_le_bytes([frame[23], frame[24], frame[25], frame[26]]) as usize;

        if header_len + data_len > frame.len() {
            continue;
        }
        packets.push(Packet {
            bus,
            device,
            endpoint,
            transfer,
            payload: frame[header_len..header_len + data_len].to_vec(),
        });
    }
    Ok(packets)
}

/// Normalise one HID report to the 64-byte form the protocol uses.
///
/// On Windows the HID stack hands back a leading report id byte for unnumbered
/// reports; the USB capture shows 64-byte payloads, so drop it when present.
fn normalise(payload: &[u8]) -> Option<[u8; REPORT_SIZE]> {
    let body = match payload {
        [0x00, rest @ ..] if rest.len() == REPORT_SIZE => rest,
        _ if payload.len() == REPORT_SIZE => payload,
        _ => return None,
    };
    let mut buf = [0u8; REPORT_SIZE];
    buf.copy_from_slice(body);
    Some(buf)
}

/// All 64-byte HID reports the amp sent in a capture, in order.
pub fn amp_reports(path: &Path) -> Result<Vec<[u8; REPORT_SIZE]>> {
    let mut out = Vec::new();
    for packet in read_packets(path)? {
        if !packet.is_interrupt() || !packet.is_in() {
            continue;
        }
        if let Some(report) = normalise(&packet.payload) {
            out.push(report);
        }
    }
    Ok(out)
}

/// Same as [`amp_reports`] but for an in-memory capture.
pub fn amp_reports_from_bytes(data: &[u8]) -> Result<Vec<[u8; REPORT_SIZE]>> {
    let mut out = Vec::new();
    for packet in packets_from_bytes(data)? {
        if !packet.is_interrupt() || !packet.is_in() {
            continue;
        }
        if let Some(report) = normalise(&packet.payload) {
            out.push(report);
        }
    }
    Ok(out)
}

/// All 64-byte HID reports in a capture, both directions.
pub fn all_reports(path: &Path) -> Result<Vec<(bool, [u8; REPORT_SIZE])>> {
    let mut out = Vec::new();
    for packet in read_packets(path)? {
        if !packet.is_interrupt() {
            continue;
        }
        if let Some(report) = normalise(&packet.payload) {
            out.push((packet.is_in(), report));
        }
    }
    Ok(out)
}
