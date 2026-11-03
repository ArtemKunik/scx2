use super::*;

/// One decoded 64-byte state report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    raw: [u8; REPORT_SIZE],
}

impl Message {
    /// Wrap a raw report, if it is an amp state frame.
    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.len() != REPORT_SIZE || raw[0] != FRAME_MARKER || raw[1] != AMP_TO_HOST {
            return None;
        }
        let mut buf = [0u8; REPORT_SIZE];
        buf.copy_from_slice(raw);
        Some(Message { raw: buf })
    }

    pub fn from_bytes(raw: [u8; REPORT_SIZE]) -> Option<Self> {
        Self::parse(&raw)
    }

    pub fn as_bytes(&self) -> &[u8; REPORT_SIZE] {
        &self.raw
    }

    pub fn byte(&self, offset: usize) -> u8 {
        self.raw[offset]
    }

    /// Overwrite one byte. Used by the editor and by the tests.
    pub fn set_byte(&mut self, offset: usize, value: u8) {
        self.raw[offset] = value;
    }

    pub fn kind(&self) -> Kind {
        Kind::from_code(self.raw[2])
    }

    pub fn bank(&self) -> u8 {
        self.raw[3]
    }

    pub fn index(&self) -> u8 {
        self.raw[4]
    }

    pub fn flag(&self) -> u8 {
        self.raw[7]
    }

    pub fn model(&self) -> Option<u16> {
        self.kind()
            .has_model()
            .then(|| u16::from_le_bytes([self.raw[OFF_MODEL], self.raw[OFF_MODEL + 1]]))
    }

    pub fn slot(&self) -> Option<u8> {
        self.kind().has_slot().then(|| self.raw[OFF_SLOT])
    }

    pub fn params(&self) -> &[u8] {
        &self.raw[OFF_PARAMS..]
    }

    pub fn name(&self) -> String {
        if self.kind() != Kind::Name {
            return String::new();
        }
        let field = &self.raw[OFF_NAME..OFF_NAME + NAME_LEN];
        let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        String::from_utf8_lossy(&field[..end]).trim().to_string()
    }

    /// Human readable one-line summary.
    pub fn summary(&self) -> String {
        let mut out = format!(
            "{} bank={} idx={}",
            self.kind().label(),
            self.bank(),
            self.index()
        );
        if self.kind() == Kind::Name {
            out.push_str(&format!(" name={:?}", self.name()));
        } else if let Some(model) = self.model() {
            out.push_str(&format!(" model=0x{model:04x}"));
            if let Some(slot) = self.slot() {
                out.push_str(&format!(" slot={slot}"));
            }
        }
        out
    }
}
