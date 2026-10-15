//! Covers for the receiver: JPEG at most 600 px a side. Receivers reject
//! PNG, and Spotify hands out PNG.

use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapEncoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform,
    ColorManagementMode, ExifOrientationMode,
};
use windows::Storage::Streams::{DataReader, DataWriter, IRandomAccessStream, InMemoryRandomAccessStream};
use windows::core::{Interface, Result};

/// What Apple senders push; MediaRemote caps what it accepts.
const SIDE: u32 = 600;

pub fn read_stream(stream: &IRandomAccessStream) -> Result<Vec<u8>> {
    let size = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    reader.LoadAsync(size)?.join()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

fn memory_stream(bytes: &[u8]) -> Result<InMemoryRandomAccessStream> {
    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.join()?;
    writer.DetachStream()?;
    stream.Seek(0)?;
    Ok(stream)
}

/// Any image Windows decodes, as a JPEG that fits in 600 x 600.
pub fn to_jpeg(image: &[u8]) -> Option<Vec<u8>> {
    let encode = || -> Result<Vec<u8>> {
        let decoder = BitmapDecoder::CreateAsync(&memory_stream(image)?)?.join()?;
        let (width, height) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
        let transform = BitmapTransform::new()?;
        let scale = (SIDE as f64 / width.max(height) as f64).min(1.0);
        transform.SetScaledWidth(((width as f64 * scale).round() as u32).max(1))?;
        transform.SetScaledHeight(((height as f64 * scale).round() as u32).max(1))?;
        transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
        let bitmap = decoder
            .GetSoftwareBitmapTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Ignore,
                &transform,
                ExifOrientationMode::RespectExifOrientation,
                ColorManagementMode::ColorManageToSRgb,
            )?
            .join()?;
        let out = InMemoryRandomAccessStream::new()?;
        let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::JpegEncoderId()?, &out)?.join()?;
        encoder.SetSoftwareBitmap(&bitmap)?;
        encoder.FlushAsync()?.join()?;
        read_stream(&out.cast()?)
    };
    encode().ok()
}
