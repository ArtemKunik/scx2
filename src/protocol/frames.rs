use super::*;

// ---------------------------------------------------------------------------
// host -> amp commands
// ---------------------------------------------------------------------------

fn report(bytes: &[u8]) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    let n = bytes.len().min(REPORT_SIZE);
    buf[..n].copy_from_slice(&bytes[..n]);
    buf
}

/// First message FUSE sends after opening the device.
pub fn handshake_step1() -> [u8; REPORT_SIZE] {
    report(&[0x00, HOST_CONTROL])
}

/// Second message; `0xc1` selects the v2/SCX2 command dialect.
pub fn handshake_step2() -> [u8; REPORT_SIZE] {
    report(&[0x1A, HOST_REQUEST])
}

/// Ask the amp for a full parameter dump. It answers with a stream of state
/// reports and closes with [`END_OF_DUMP`].
pub fn request_dump() -> [u8; REPORT_SIZE] {
    report(&[0xFF, HOST_REQUEST])
}

/// Recall a stored preset. Mirrors the Mustang patch-change frame; not yet
/// verified on SCX2 hardware, kept for the probe tools.
pub fn patch_change(slot: u8) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = FRAME_MARKER;
    buf[1] = AMP_TO_HOST;
    buf[2] = 0x01;
    buf[4] = slot;
    buf[6] = 0x01;
    buf
}

/// The Mustang "execute" frame, sent after a model/preset frame.
pub fn execute_model_change() -> [u8; REPORT_SIZE] {
    report(&[0x1C, 0x03])
}

/// Turn an amp report into a host report, keeping the payload untouched.
pub fn as_host_frame(raw: &[u8; REPORT_SIZE], direction: u8) -> [u8; REPORT_SIZE] {
    let mut buf = *raw;
    buf[1] = direction;
    buf
}

/// Parse a list of hex bytes, e.g. `"1c 01 05 ff"` or `"1c0105ff"`.
pub fn parse_hex(text: &str) -> Option<Vec<u8>> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != ',')
        .collect();
    if !cleaned.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
        .collect();
    bytes
}

/// The two reports the amp sends during the handshake carry a build banner,
/// e.g. `May 30 2013`, at offset 4.
pub fn boot_banner(raw: &[u8]) -> Option<String> {
    let field = raw.get(4..18)?;
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let text = String::from_utf8_lossy(&field[..end]).trim().to_string();
    text.chars()
        .next()
        .filter(char::is_ascii_alphabetic)
        .map(|_| text)
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
