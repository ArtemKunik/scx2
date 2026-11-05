//! Hardware transport over hidapi.
//!
//! FUSE used hidapi on the same HID interface, so we do too: no libusb driver
//! swapping, no Zadig, the Windows in-box HID driver is enough.

use std::ffi::CString;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use hidapi::{HidApi, HidDevice};

use crate::protocol::{self, PRODUCT_ID, REPORT_SIZE, VENDOR_ID};

/// Extra advice appended to "could not open the amp", per platform.
#[cfg(target_os = "linux")]
const ACCESS_HINT: &str = "\n- on Linux the amp is a /dev/hidraw node: install \
     packaging/udev/70-scx2.rules so it is readable without root, then replug";
#[cfg(not(target_os = "linux"))]
const ACCESS_HINT: &str = "";

/// A device as reported by the HID enumerator.
#[derive(Debug, Clone)]
pub struct DeviceSummary {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// HID interface number. Linux exposes one hidraw node per interface, so
    /// a composite device can appear more than once.
    pub interface_number: i32,
    /// HID usage page and usage; the vendor collection is the one that
    /// carries the preset reports.
    pub usage_page: u16,
    pub usage: u16,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub serial: Option<String>,
}

impl DeviceSummary {
    pub fn label(&self) -> String {
        let name = self
            .product
            .as_deref()
            .unwrap_or("unknown device")
            .trim()
            .to_string();
        format!(
            "{name}  [{:04x}:{:04x}]  {}",
            self.vendor_id, self.product_id, self.path
        )
    }
}

/// Enumerate HID devices. With `fender_only`, keep the Fender vendor id.
pub fn scan(fender_only: bool) -> Result<Vec<DeviceSummary>> {
    let api = HidApi::new().context("initialising hidapi")?;
    let devices = api
        .device_list()
        .filter(|d| !fender_only || d.vendor_id() == VENDOR_ID)
        .map(|d| DeviceSummary {
            path: d.path().to_string_lossy().into_owned(),
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            interface_number: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            product: d.product_string().map(str::to_string),
            manufacturer: d.manufacturer_string().map(str::to_string),
            serial: d.serial_number().map(str::to_string),
        })
        .collect();
    Ok(devices)
}

/// Find the amp, preferring `path` when given.
fn open_device(path: Option<&str>) -> Result<HidDevice> {
    let api = HidApi::new().context("initialising hidapi")?;
    let device = match path {
        Some(path) => {
            let c_path = CString::new(path).context("invalid device path")?;
            api.open_path(&c_path)
                .with_context(|| format!("opening HID device {path}"))?
        }
        None => {
            // A composite device can expose more than one HID collection, and
            // some systems list the vendor node after a consumer-control one.
            // Open the vendor-defined usage page explicitly when it is known,
            // then fall back to the plain VID/PID lookup used on Windows.
            match open_vendor_interface(&api) {
                Some(device) => device,
                None => api.open(VENDOR_ID, PRODUCT_ID).map_err(|error| {
                    anyhow!(
                        "could not open the amp [{VENDOR_ID:04x}:{PRODUCT_ID:04x}]: {error}\n\
                         - is the amp powered on and connected with a data cable?\n\
                         - close Fender FUSE or anything else holding the device\n\
                         - `scx2 scan --all` lists every HID device{ACCESS_HINT}"
                    )
                })?,
            }
        }
    };
    Ok(device)
}

/// The amp's vendor-defined HID collection, if the platform reports one.
///
/// Windows usually exposes a single interface, so the fallback keeps the
/// historical behaviour. Linux can expose several hidraw nodes for the same
/// device; preferring `usage_page >= 0xff00` picks the one that carries the
/// 64-byte preset reports.
fn open_vendor_interface(api: &HidApi) -> Option<HidDevice> {
    api.device_list()
        .filter(|info| info.vendor_id() == VENDOR_ID && info.product_id() == PRODUCT_ID)
        .filter(|info| info.usage_page() >= 0xff00)
        .min_by_key(|info| info.interface_number())
        .and_then(|info| api.open_path(info.path()).ok())
}

/// An open amp connection.
pub struct Amp {
    device: HidDevice,
}

impl Amp {
    pub fn open(path: Option<&str>) -> Result<Self> {
        let device = open_device(path)?;
        Ok(Amp { device })
    }

    /// Send one 64-byte report.
    ///
    /// hidapi expects the report id as the first byte; the amp uses a single
    /// unnumbered report, so the id is 0x00 and the buffer is 65 bytes.
    pub fn write_report(&self, report: &[u8; REPORT_SIZE]) -> Result<()> {
        let mut buf = [0u8; REPORT_SIZE + 1];
        buf[1..].copy_from_slice(report);
        let written = self.device.write(&buf).context("writing HID report")?;
        if written != buf.len() {
            bail!("short HID write: {written} of {} bytes", buf.len());
        }
        Ok(())
    }

    /// Read one 64-byte report, if one arrives before the timeout.
    pub fn read_report(&self, timeout_ms: i32) -> Result<Option<[u8; REPORT_SIZE]>> {
        let mut buf = [0u8; REPORT_SIZE + 1];
        let n = self
            .device
            .read_timeout(&mut buf, timeout_ms)
            .context("reading HID report")?;
        if n == 0 {
            return Ok(None);
        }
        let data = &buf[..n];
        let body: &[u8] = if data.len() == REPORT_SIZE + 1 && data[0] == 0x00 {
            &data[1..]
        } else if data.len() == REPORT_SIZE {
            data
        } else {
            return Ok(None);
        };
        let mut report = [0u8; REPORT_SIZE];
        report.copy_from_slice(body);
        Ok(Some(report))
    }

    /// The two-message handshake FUSE performs before any command.
    /// Returns the firmware build banner the amp reports, when present.
    pub fn handshake(&self) -> Result<Option<String>> {
        self.write_report(&protocol::handshake_step1())?;
        let first = self.read_report(500)?;
        self.write_report(&protocol::handshake_step2())?;
        let second = self.read_report(500)?;
        Ok(second
            .or(first)
            .and_then(|report| protocol::boot_banner(&report)))
    }

    /// Ask for a full dump and collect the reports until the amp acknowledges.
    pub fn read_dump(&self) -> Result<protocol::Dump> {
        self.write_report(&protocol::request_dump())?;
        let mut dump = protocol::Dump::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match self.read_report(1000)? {
                Some(report) => {
                    dump.add_raw(&report);
                    if dump.ended {
                        break;
                    }
                }
                None => continue,
            }
        }
        if !dump.ended {
            return Err(anyhow!(
                "dump did not finish: the amp stopped sending without an acknowledgement"
            ));
        }
        Ok(dump)
    }

    /// One-line description of the open device.
    pub fn describe(&self) -> Result<String> {
        let info = self.device.get_device_info().context("querying device")?;
        Ok(format!(
            "{} [{:04x}:{:04x}]",
            info.product_string().unwrap_or("Super Champ X2"),
            info.vendor_id(),
            info.product_id()
        ))
    }
}
