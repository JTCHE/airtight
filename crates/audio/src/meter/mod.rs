//! The endpoint's peak level. Reading it costs nothing, where a capture
//! stream would poll all day.

use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::core::Result;

pub struct Meter(IAudioMeterInformation);

impl Meter {
    pub fn open(endpoint_id: &str) -> Result<Meter> {
        let device = crate::endpoint::device(endpoint_id)?;
        Ok(Meter(unsafe { device.Activate(CLSCTX_ALL, None)? }))
    }

    /// 0.0 to 1.0. Fails once the endpoint is removed or disabled.
    pub fn peak(&self) -> Result<f32> {
        unsafe { self.0.GetPeakValue() }
    }
}
