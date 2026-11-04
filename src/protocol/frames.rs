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
    recall_preset(0, slot)
}

/// Recall a preset from a specific bank. Still the Mustang frame shape; the
/// SCX2 has not been shown to accept it.
pub fn recall_preset(bank: u8, slot: u8) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = FRAME_MARKER;
    buf[1] = AMP_TO_HOST;
    buf[2] = 0x01;
    buf[3] = bank;
    buf[4] = slot;
    buf[6] = 0x01;
    buf
}

/// Build a `0x04` name block. The caller sends it with the host direction and
/// the execute frame. Whether a stored slot actually accepts it is untested.
pub fn name_change(bank: u8, index: u8, name: &str) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = FRAME_MARKER;
    buf[1] = AMP_TO_HOST;
    buf[2] = super::Kind::Name.code();
    buf[3] = bank;
    buf[4] = index;
    buf[6] = 0x01;
    for (index, byte) in name.as_bytes().iter().take(NAME_LEN).enumerate() {
        buf[OFF_NAME + index] = *byte;
    }
    buf
}

/// Point an existing block at another bank/index, keeping its payload.
///
/// This is the building block for the experimental `store` command: the amp
/// already accepts a block write, so the only open question is whether writing
/// every block with the target coordinates is enough to persist a preset.
pub fn retarget(raw: &[u8; REPORT_SIZE], bank: u8, index: u8) -> [u8; REPORT_SIZE] {
    let mut buf = *raw;
    buf[3] = bank;
    buf[4] = index;
    buf
}

/// FUSE's save-preset frame.
///
/// The amp stores its **current live buffer** in `bank`/`index` under `name`;
/// there is no per-block payload to send. This is the frame the `refuse`
/// project calls `savePreset`, which in turn mirrors FUSE 2.7.1:
///
/// ```text
/// 1c 01 03 <bank> <index> 00 01 01 <name at 16..48>
/// ```
///
/// FUSE writes `0` in byte 3 because a Mustang has one flat preset list. The
/// SCX2 has bank 1 (amp) and bank 2 (FX), and byte 3 selects the bank:
/// verified on hardware by saving into `fx 15` and reading the slot back.
pub fn save_preset(bank: u8, index: u8, name: &str) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = FRAME_MARKER;
    buf[1] = 0x01; // DATA_READ
    buf[2] = 0x03; // save-preset type
    buf[3] = bank;
    buf[4] = index;
    buf[5] = 0x00;
    buf[6] = 0x01;
    buf[7] = 0x01;
    for (index, byte) in name.as_bytes().iter().take(NAME_LEN).enumerate() {
        buf[OFF_NAME + index] = *byte;
    }
    buf
}

/// The Mustang "execute" frame, sent after a model/preset frame.
pub fn execute_model_change() -> [u8; REPORT_SIZE] {
    report(&[0x1C, 0x03])
}

/// FUSE's fast bypass frame: `19 c3 <family> <state> <slot>`.
///
/// `family` is the DSP kind minus 3 (stomp 3, mod 4, delay 5, reverb 6) and
/// `state` is 1 for bypassed, 0 for enabled. Verified on hardware: toggling
/// the live reverb changed block offset 22 and toggling it back restored it.
pub fn bypass(kind: super::Kind, slot: u8, bypassed: bool) -> [u8; REPORT_SIZE] {
    let family = match kind {
        super::Kind::Stomp => 3,
        super::Kind::Mod => 4,
        super::Kind::Delay => 5,
        super::Kind::Reverb => 6,
        _ => 0,
    };
    report(&[0x19, HOST_CONTROL, family, u8::from(bypassed), slot])
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
