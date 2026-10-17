//! The delay an output reports to apps, computed the way Chrome does it:
//! frames written minus the position IAudioClock says has played.
//! `cargo run -p airtight-audio --example delay -- [name part]`

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use std::time::{Duration, Instant};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

    let wanted = std::env::args().nth(1);
    let endpoint = airtight_audio::endpoint::find(wanted.as_deref()).expect("no output");
    println!("output: {}", endpoint.name);
    airtight_audio::com();
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDevice(&windows::core::HSTRING::from(endpoint.id))?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format = client.GetMixFormat()?;
        let rate = (*format).nSamplesPerSec as u64;
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 1_000_000, 0, format, None)?;
        let buffer_frames = client.GetBufferSize()?;
        let render: IAudioRenderClient = client.GetService()?;
        let clock: IAudioClock = client.GetService()?;
        let frequency = clock.GetFrequency()?;
        println!("stream latency: {} ms", client.GetStreamLatency()? / 10_000);
        let mut written = 0u64;
        client.Start()?;
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(6) {
            let free = buffer_frames - client.GetCurrentPadding()?;
            if free > 0 {
                render.GetBuffer(free)?;
                render.ReleaseBuffer(free, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
                written += u64::from(free);
            }
            let (mut position, mut qpc) = (0u64, 0u64);
            clock.GetPosition(&mut position, Some(&mut qpc))?;
            let played = rate * position / frequency;
            if started.elapsed().as_millis() % 500 < 10 {
                println!("{:>5} ms: written {written}, played {played}, delay {} ms", started.elapsed().as_millis(), (written.saturating_sub(played)) * 1000 / rate);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        client.Stop()?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
