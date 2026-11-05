//! Windows capture endpoints: what exists, and switching the amp's input on.
//!
//! Windows can hide a capture endpoint ("disabled" in the Sound control panel).
//! WASAPI — and therefore cpal — cannot open it then, even though the device is
//! still installed. `IMMDeviceEnumerator` with the full state mask sees it
//! anyway, and the undocumented `IPolicyConfig` interface flips the same switch
//! as the panel's Enable item.
//!
//! Linux has no equivalent switch: ALSA exposes the amp as soon as the USB
//! device is plugged in, and PulseAudio/PipeWire publish it to cpal. The tuner
//! only needs to rescan its input list there.

use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{anyhow, Result};

/// Whether this platform has a capture endpoint the app can switch on.
///
/// Only Windows needs the "Enable amp input" action; see the module docs.
pub fn can_enable() -> bool {
    cfg!(windows)
}

/// A capture endpoint as Windows sees it, including hidden ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureEndpoint {
    /// Endpoint ID, in the `{0.0.0.00000000}.{...}` form.
    pub id: String,
    /// Friendly name, e.g. `Internal AUX Jack (SuperChamp X2 Amp)`.
    pub name: String,
    /// `DEVICE_STATE_*` bits. The high bit marks an endpoint the user hid.
    pub state: u32,
}

impl CaptureEndpoint {
    /// Enabled and therefore openable through WASAPI.
    pub fn is_active(&self) -> bool {
        self.state & 0x1 != 0 && self.state & 0x1000_0000 == 0
    }
}

/// Every capture endpoint Windows knows about, active or not.
pub fn capture_endpoints() -> Result<Vec<CaptureEndpoint>> {
    #[cfg(windows)]
    {
        win::capture_endpoints()
    }
    #[cfg(not(windows))]
    {
        Err(anyhow!("audio endpoints can only be inspected on Windows"))
    }
}

/// The amp's USB capture endpoint, whether or not it is enabled.
pub fn amp_endpoint() -> Result<Option<CaptureEndpoint>> {
    Ok(capture_endpoints()?
        .into_iter()
        .find(|endpoint| crate::tuner::is_amp_input(&endpoint.name)))
}

/// Show or hide one capture endpoint, like the Sound panel's Enable item.
pub fn set_endpoint_enabled(id: &str, enabled: bool) -> Result<()> {
    #[cfg(windows)]
    {
        win::set_endpoint_enabled(id, enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = (id, enabled);
        Err(anyhow!("audio endpoints can only be changed on Windows"))
    }
}

/// Switch the amp's USB input on if Windows had it disabled.
///
/// Returns the endpoint name so the caller can say what happened. Windows
/// publishes the change asynchronously, so this waits before returning.
pub fn enable_amp_input() -> Result<String> {
    let endpoint = amp_endpoint()?.ok_or_else(|| {
        anyhow!(
            "Windows does not list the amp's USB input; check the USB cable and that the amp is on"
        )
    })?;
    if !endpoint.is_active() {
        set_endpoint_enabled(&endpoint.id, true)?;
        std::thread::sleep(Duration::from_millis(800));
    }
    Ok(endpoint.name)
}

/// A background attempt to switch the amp input on, so the click cannot stall
/// a frame while Windows reconfigures the audio service.
pub struct EnableJob {
    result: Receiver<Result<String>>,
}

impl EnableJob {
    pub fn start() -> Self {
        let (sender, result) = mpsc::channel();
        let reply = sender.clone();
        let spawned = std::thread::Builder::new()
            .name("scx2-amp-input".into())
            .spawn(move || {
                let _ = sender.send(enable_amp_input());
            });
        if let Err(error) = spawned {
            let _ = reply.send(Err(anyhow!("could not start the device thread: {error}")));
        }
        EnableJob { result }
    }

    /// `Some` once the attempt has finished.
    pub fn poll(&self) -> Option<Result<String>> {
        self.result.try_recv().ok()
    }
}

#[cfg(windows)]
mod win {
    use super::*;

    use anyhow::Context;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::{
        eCapture, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE,
        DEVICE_STATEMASK_ALL,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
        STGM_READ,
    };

    /// `IPolicyConfig` is undocumented, so the windows crate has no binding.
    const CLSID_POLICY_CONFIG_CLIENT: GUID =
        GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
    /// The Windows 7+ interface; the Vista one has one method slot fewer.
    const IID_POLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);

    #[link(name = "ole32")]
    extern "system" {
        #[link_name = "CoCreateInstance"]
        fn create_instance(
            rclsid: *const GUID,
            punkouter: *mut std::ffi::c_void,
            dwclscontext: u32,
            riid: *const GUID,
            ppv: *mut *mut std::ffi::c_void,
        ) -> windows::core::HRESULT;
    }

    type SetEndpointVisibility =
        unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, i32) -> windows::core::HRESULT;
    type Release = unsafe extern "system" fn(*mut std::ffi::c_void) -> u32;

    pub(super) fn capture_endpoints() -> Result<Vec<CaptureEndpoint>> {
        // SAFETY: every call below follows the documented WASAPI lifetime
        // rules; the enumerator and collection are released on drop.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .context("opening the Windows audio device enumerator")?;
            let collection = enumerator
                .EnumAudioEndpoints(eCapture, DEVICE_STATE(DEVICE_STATEMASK_ALL))
                .context("listing capture endpoints")?;
            let count = collection
                .GetCount()
                .context("counting capture endpoints")?;
            let mut endpoints = Vec::with_capacity(count as usize);
            for index in 0..count {
                let device = collection
                    .Item(index)
                    .context("reading a capture endpoint")?;
                endpoints.push(describe(&device)?);
            }
            Ok(endpoints)
        }
    }

    unsafe fn describe(device: &IMMDevice) -> Result<CaptureEndpoint> {
        let mut state = 0u32;
        let _ = device.GetState(&mut state);

        let id = device.GetId().context("reading an endpoint ID")?;
        let id_text = id.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id.0 as *const _));

        let name = device
            .OpenPropertyStore(STGM_READ)
            .ok()
            .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName).ok())
            .map(|value| value.to_string())
            .unwrap_or_default();

        Ok(CaptureEndpoint {
            id: id_text,
            name,
            state,
        })
    }

    pub(super) fn set_endpoint_enabled(id: &str, enabled: bool) -> Result<()> {
        // SAFETY: the object is a live COM reference for the whole call, and
        // the vtable layout of IPolicyConfig is fixed since Windows 7:
        // three IUnknown slots, eleven policy methods, then this one.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let mut raw: *mut std::ffi::c_void = std::ptr::null_mut();
            create_instance(
                &CLSID_POLICY_CONFIG_CLIENT,
                std::ptr::null_mut(),
                CLSCTX_ALL.0,
                &IID_POLICY_CONFIG,
                &mut raw,
            )
            .ok()
            .context("opening the Windows audio policy client")?;
            if raw.is_null() {
                return Err(anyhow!("the audio policy client returned no interface"));
            }

            let vtable = *(raw as *mut *const *const std::ffi::c_void);
            let set_visibility: SetEndpointVisibility = std::mem::transmute(*vtable.add(14));
            let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            let result = set_visibility(raw, PCWSTR::from_raw(wide.as_ptr()), i32::from(enabled))
                .ok()
                .context("switching the input on");

            let release: Release = std::mem::transmute(*vtable.add(2));
            release(raw);
            result
        }
    }
}
