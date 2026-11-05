//! Reader for USB captures (`.pcap`).
//!
//! Two formats are understood: USBPcap (link type 249, what FUSE sessions on
//! Windows produce) and Linux `usbmon` in the mmapped format (link type 220).
//! Handy for offline work: a capture made while FUSE or `scx2` was talking to
//! the amp replays through exactly the same decoder the hardware path uses.
//! `captures/` ships one USBPcap recording.
//!
//! Linux captures need `modprobe usbmon` and `tshark`/`dumpcap`; see
//! `docs/linux.md`.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::protocol::REPORT_SIZE;

/// USBPcap's libpcap link type.
pub const LINKTYPE_USBPCAP: u32 = 249;
/// Linux `usbmon`'s mmapped link type (`DLT_USB_LINUX_MMAPPED`).
pub const LINKTYPE_USB_LINUX_MMAPPED: u32 = 220;

/// `struct mon_bin_hdr` from `linux/usbdevice_fs.h`, for the mmapped format.
const USB_MON_HEADER: usize = 64;
/// One isochronous descriptor, when `ndesc` is non-zero.
const USB_MON_DESCRIPTOR: usize = 16;

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
    fn u16(&self, off: usize) -> u16 {
        let bytes = [self.data[off], self.data[off + 1]];
        if self.big_endian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        }
    }

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

/// Parse every packet from an in-memory capture.
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
    match reader.u32(20) {
        LINKTYPE_USBPCAP => packets_usbpcap(&reader),
        LINKTYPE_USB_LINUX_MMAPPED => packets_usbmon(&reader),
        other => bail!(
            "unsupported capture link type {other}; expected {LINKTYPE_USBPCAP} \
             (USBPcap) or {LINKTYPE_USB_LINUX_MMAPPED} (Linux usbmon)"
        ),
    }
}

/// USBPcap frames: a 27-byte header carries the bus, device, endpoint,
/// transfer type and payload length.
fn packets_usbpcap(reader: &Reader) -> Result<Vec<Packet>> {
    let data = reader.data;
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

/// Linux `usbmon` frames: a `mon_bin_hdr` with the transfer metadata, then
/// the payload. Only completion packets (`C`) carry data; submit packets are
/// metadata only and are skipped.
fn packets_usbmon(reader: &Reader) -> Result<Vec<Packet>> {
    let data = reader.data;
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
        if frame.len() < USB_MON_HEADER {
            continue;
        }

        // 'C' is a transfer completion, which is where USB payloads appear.
        if frame[8] != b'C' {
            continue;
        }
        let frame_reader = Reader {
            data: frame,
            big_endian: reader.big_endian,
        };
        let ndesc = frame_reader.u32(60) as usize;
        let header_len = USB_MON_HEADER + ndesc * USB_MON_DESCRIPTOR;
        if header_len > frame.len() {
            continue;
        }
        let data_len = frame_reader.u32(36) as usize;
        let payload = &frame[header_len..];
        let data_len = data_len.min(payload.len());

        packets.push(Packet {
            bus: frame_reader.u16(12),
            device: frame[11] as u16,
            endpoint: frame[10],
            transfer: frame[9],
            payload: payload[..data_len].to_vec(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One little-endian usbmon file with a single completion frame.
    fn usbmon_capture(transfer: u8, endpoint: u8, payload: &[u8]) -> Vec<u8> {
        let mut file = Vec::new();
        file.extend_from_slice(&MAGIC_US.to_le_bytes());
        file.extend_from_slice(&2u16.to_le_bytes());
        file.extend_from_slice(&4u16.to_le_bytes());
        file.extend_from_slice(&0i32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&65535u32.to_le_bytes());
        file.extend_from_slice(&LINKTYPE_USB_LINUX_MMAPPED.to_le_bytes());

        let frame_len = USB_MON_HEADER + payload.len();
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&(frame_len as u32).to_le_bytes());
        file.extend_from_slice(&(frame_len as u32).to_le_bytes());

        let mut header = [0u8; USB_MON_HEADER];
        header[8] = b'C';
        header[9] = transfer;
        header[10] = endpoint;
        header[11] = 5;
        header[12..14].copy_from_slice(&1u16.to_le_bytes());
        header[36..40].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        file.extend_from_slice(&header);
        file.extend_from_slice(payload);
        file
    }

    #[test]
    fn reads_linux_usbmon_interrupt_reports() {
        let payload: Vec<u8> = (0..REPORT_SIZE as u8).collect();
        let file = usbmon_capture(TRANSFER_INTERRUPT, 0x81, &payload);
        let packets = packets_from_bytes(&file).unwrap();
        assert_eq!(packets.len(), 1);
        assert!(packets[0].is_in());
        assert!(packets[0].is_interrupt());
        assert_eq!(packets[0].bus, 1);
        assert_eq!(packets[0].device, 5);
        assert_eq!(packets[0].payload, payload);

        let mut expected = [0u8; REPORT_SIZE];
        expected.copy_from_slice(&payload);
        assert_eq!(amp_reports_from_bytes(&file).unwrap(), vec![expected]);
    }

    #[test]
    fn skips_usbmon_submit_packets() {
        let payload = [0u8; REPORT_SIZE];
        let mut file = usbmon_capture(TRANSFER_INTERRUPT, 0x81, &payload);
        // The frame starts after the 24-byte file header and 16-byte record.
        file[24 + 16 + 8] = b'S';
        assert!(packets_from_bytes(&file).unwrap().is_empty());
    }

    #[test]
    fn rejects_unknown_link_types() {
        let mut file = usbmon_capture(TRANSFER_INTERRUPT, 0x81, &[]);
        file[20..24].copy_from_slice(&1u32.to_le_bytes());
        let error = packets_from_bytes(&file).unwrap_err().to_string();
        assert!(error.contains("unsupported capture link type"));
    }
}
