//! The active render endpoints, by id and the name the Sound settings show.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::core::{HSTRING, Result};

use crate::Endpoint;

fn enumerator() -> Result<IMMDeviceEnumerator> {
    crate::com();
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

fn describe(device: &IMMDevice) -> Result<Endpoint> {
    unsafe {
        let raw = device.GetId()?;
        let id = raw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(raw.0 as *const _));
        let name = device.OpenPropertyStore(STGM_READ)?.GetValue(&PKEY_Device_FriendlyName)?.to_string();
        Ok(Endpoint { id, name })
    }
}

pub fn list() -> Vec<Endpoint> {
    let found = || -> Result<Vec<Endpoint>> {
        let devices = unsafe { enumerator()?.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)? };
        let count = unsafe { devices.GetCount()? };
        Ok((0..count).filter_map(|i| unsafe { devices.Item(i) }.ok()).filter_map(|d| describe(&d).ok()).collect())
    };
    found().unwrap_or_default()
}

pub fn default() -> Option<Endpoint> {
    let device = unsafe { enumerator().ok()?.GetDefaultAudioEndpoint(eRender, eConsole).ok()? };
    describe(&device).ok()
}

/// The saved endpoint by name, a near match, or else the default output.
pub fn find(name: Option<&str>) -> Option<Endpoint> {
    let all = list();
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        let lower = name.to_lowercase();
        let hit = all.iter().find(|e| e.name == name).or_else(|| all.iter().find(|e| e.name.to_lowercase().contains(&lower)));
        if let Some(hit) = hit {
            return Some(hit.clone());
        }
    }
    default().or_else(|| all.into_iter().next())
}

pub(crate) fn device(id: &str) -> Result<IMMDevice> {
    unsafe { enumerator()?.GetDevice(&HSTRING::from(id)) }
}
