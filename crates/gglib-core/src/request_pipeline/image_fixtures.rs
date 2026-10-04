//! Minimal PNG and JPEG files, built byte by byte, for the tests that read
//! an image's size or price a request by it. Each is a real header and
//! nothing a decoder could draw.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

/// A PNG's signature and its `IHDR` chunk, for `width` by `height`.
pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    // Bit depth, colour type, compression, filter, interlace; then the CRC.
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

/// One JPEG segment: `0xFF`, `marker`, the length, `payload`.
pub(crate) fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
    let len = u16::try_from(payload.len() + 2).expect("a segment holds at most 65,533 bytes");
    let mut bytes = vec![0xFF, marker];
    bytes.extend(len.to_be_bytes());
    bytes.extend(payload);
    bytes
}

/// A frame header for `width` by `height`, under the marker `sof`.
pub(crate) fn frame(sof: u8, width: u16, height: u16) -> Vec<u8> {
    let mut payload = vec![8];
    payload.extend(height.to_be_bytes());
    payload.extend(width.to_be_bytes());
    // Three components, each an id, a sampling factor and a table.
    payload.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    segment(sof, &payload)
}

/// A JPEG: the start of image, a JFIF `APP0`, `before` (whole segments, as
/// bytes), the frame header under `sof`, and a start of scan.
pub(crate) fn jpeg_with(sof: u8, width: u16, height: u16, before: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8];
    bytes.extend(segment(0xE0, b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0"));
    bytes.extend(before);
    bytes.extend(frame(sof, width, height));
    bytes.extend(segment(0xDA, &[3, 1, 0, 2, 0x11, 3, 0x11, 0, 0x3F, 0]));
    bytes.extend([0x00, 0xFF, 0xD9]);
    bytes
}

/// A baseline JPEG (`SOF0`) of `width` by `height`.
pub(crate) fn jpeg(width: u16, height: u16) -> Vec<u8> {
    jpeg_with(0xC0, width, height, &[])
}

/// `bytes` as a base64 data URL of type `mime`.
pub(crate) fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

/// A PNG data URL of `width` by `height` whose payload is `raw_len` bytes:
/// the header, then filler. A screenshot's worth of base64 with a real size.
pub(crate) fn png_url_of(width: u32, height: u32, raw_len: usize) -> String {
    let mut bytes = png(width, height);
    bytes.resize(raw_len.max(bytes.len()), 0xA5);
    data_url("image/png", &bytes)
}
