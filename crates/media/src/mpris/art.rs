//! Covers for the receiver. Receivers take JPEG only, and Linux has no
//! system image codec to convert with, so only JPEG covers pass.

/// The cover behind an MPRIS art URL. Only local files: most players
/// cache their covers there.
// ponytail: no http(s) covers (Spotify's); they need a TLS client.
pub fn load(url: &str) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    let path = unescape(url.strip_prefix("file://")?);
    std::fs::read(std::ffi::OsStr::from_bytes(&path)).ok()
}

/// Undo the %XX escapes of a URL path.
fn unescape(path: &str) -> Vec<u8> {
    let (bytes, mut out, mut i) = (path.as_bytes(), Vec::new(), 0);
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%').then(|| path.get(i + 1..i + 3)).flatten().and_then(|h| u8::from_str_radix(h, 16).ok());
        match escaped {
            Some(byte) => (out.push(byte), i += 3),
            None => (out.push(bytes[i]), i += 1),
        };
    }
    out
}

/// The cover as is when it is a JPEG.
pub fn to_jpeg(image: &[u8]) -> Option<Vec<u8>> {
    image.starts_with(&[0xff, 0xd8, 0xff]).then(|| image.to_vec())
}

#[cfg(test)]
mod tests {
    #[test]
    fn unescapes_a_file_url() {
        assert_eq!(super::unescape("/tmp/My%20Cover.jpg"), b"/tmp/My Cover.jpg");
        assert_eq!(super::unescape("/a%2"), b"/a%2");
    }
}
