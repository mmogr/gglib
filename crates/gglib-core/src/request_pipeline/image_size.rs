//! An image's pixel size, read from its header alone.
//!
//! [`super::images`] prices an image by its size, and the size of a PNG or a
//! JPEG is in its first bytes: a PNG's `IHDR` chunk is the first thing after
//! the signature, and a JPEG's frame header follows whatever metadata
//! segments the camera or the editor wrote. Nothing here decodes pixels, and
//! nothing here decodes a whole base64 payload: a data URL gives up a prefix,
//! and a longer one only while the header says it has not ended.
//!
//! Every input is a client's, so every read is bounds-checked. Bytes that are
//! cut short, or are not an image, are `None` and never a panic.

use base64::Engine as _;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{self, GeneralPurpose};

/// The most of an image that is decoded to find its size. A JPEG may carry
/// several 64 KiB metadata segments (EXIF, an ICC profile, a thumbnail)
/// before its frame header; 1 MiB holds sixteen of them.
pub const MAX_HEADER_BYTES: usize = 1 << 20;

/// The first prefix decoded. A PNG needs 24 bytes and most JPEGs a few
/// hundred; the prefix doubles from here while the header runs on.
const FIRST_PREFIX_BYTES: usize = 4096;

/// The eight bytes every PNG starts with.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The two bytes every JPEG starts with: the start-of-image marker.
const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];

/// Standard base64, read with or without its trailing `=`.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    general_purpose::PAD.with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// What the bytes in hand say about the image they start.
#[derive(Debug, PartialEq, Eq)]
enum Header {
    /// Width and height, in pixels.
    Size(u32, u32),
    /// A PNG or JPEG so far, cut short before its size.
    NeedsMore,
    /// Not a PNG or a JPEG, or one whose header is not in order.
    Unreadable,
}

/// The width and height of the PNG or JPEG `bytes` starts, in pixels.
///
/// `None` for any other format, for a header cut short, and for a size of
/// zero either way.
#[must_use]
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    match read_header(bytes) {
        Header::Size(width, height) => Some((width, height)),
        Header::NeedsMore | Header::Unreadable => None,
    }
}

/// The media type a PNG is stored and served under.
pub const PNG_MIME: &str = "image/png";

/// The media type a JPEG is stored and served under.
pub const JPEG_MIME: &str = "image/jpeg";

/// The media type of the image `bytes` starts, by its first bytes alone:
/// [`PNG_MIME`] or [`JPEG_MIME`], and `None` for anything else.
#[must_use]
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&PNG_SIGNATURE) {
        Some(PNG_MIME)
    } else if bytes.starts_with(&JPEG_SOI) {
        Some(JPEG_MIME)
    } else {
        None
    }
}

/// The width and height of the image in a `data:<mime>;base64,<payload>`
/// URL, decoding only as much of the payload as its header takes, and never
/// more than [`MAX_HEADER_BYTES`].
///
/// `None` for a URL of any other kind (`http:`, `https:`, a data URL that is
/// not base64), and wherever [`image_size`] is.
#[must_use]
pub fn data_url_image_size(url: &str) -> Option<(u32, u32)> {
    let payload = base64_payload(url)?;
    let mut want = FIRST_PREFIX_BYTES;
    loop {
        let (bytes, whole) = decoded_prefix(payload, want)?;
        match read_header(&bytes) {
            Header::Size(width, height) => return Some((width, height)),
            Header::NeedsMore if !whole && want < MAX_HEADER_BYTES => {
                want = (want * 2).min(MAX_HEADER_BYTES);
            }
            Header::NeedsMore | Header::Unreadable => return None,
        }
    }
}

/// The payload of a base64 data URL: what follows its first comma.
fn base64_payload(url: &str) -> Option<&str> {
    let (meta, payload) = url.strip_prefix("data:")?.split_once(',')?;
    meta.ends_with(";base64").then_some(payload)
}

/// About the first `want` bytes `payload` encodes, and whether that is all
/// of them. The cut falls on a four-character group, so the prefix decodes
/// on its own; nothing after the cut is read.
fn decoded_prefix(payload: &str, want: usize) -> Option<(Vec<u8>, bool)> {
    let chars = (want / 3 * 4).min(payload.len());
    let prefix = payload.as_bytes().get(..chars)?;
    let bytes = BASE64.decode(prefix).ok()?;
    Some((bytes, chars == payload.len()))
}

fn read_header(bytes: &[u8]) -> Header {
    if bytes.starts_with(&JPEG_SOI) {
        jpeg(bytes)
    } else if bytes.starts_with(&PNG_SIGNATURE) {
        png(bytes)
    } else if PNG_SIGNATURE.starts_with(bytes) || JPEG_SOI.starts_with(bytes) {
        // Shorter than a signature, and a signature as far as it goes.
        Header::NeedsMore
    } else {
        Header::Unreadable
    }
}

/// A size, when neither side is zero.
const fn sized(width: u32, height: u32) -> Header {
    if width == 0 || height == 0 {
        Header::Unreadable
    } else {
        Header::Size(width, height)
    }
}

/// A PNG's size: the first chunk after the signature is `IHDR`, a four-byte
/// length and the chunk's name, then the width and the height, big-endian.
fn png(bytes: &[u8]) -> Header {
    let Some(
        &[
            _,
            _,
            _,
            _,
            b'I',
            b'H',
            b'D',
            b'R',
            w0,
            w1,
            w2,
            w3,
            h0,
            h1,
            h2,
            h3,
        ],
    ) = bytes.get(8..24)
    else {
        return if bytes.len() < 24 {
            Header::NeedsMore
        } else {
            Header::Unreadable
        };
    };
    sized(
        u32::from_be_bytes([w0, w1, w2, w3]),
        u32::from_be_bytes([h0, h1, h2, h3]),
    )
}

/// A JPEG's size: walk its segments to the first frame header.
///
/// A segment is `0xFF`, a marker byte, and, for all but a few markers, a
/// two-byte big-endian length that counts itself and the payload after it.
/// The frame header's payload is a precision byte, the height, the width.
fn jpeg(bytes: &[u8]) -> Header {
    let mut at = JPEG_SOI.len();
    loop {
        match bytes.get(at) {
            None => return Header::NeedsMore,
            Some(0xFF) => {}
            Some(_) => return Header::Unreadable,
        }
        let Some(&marker) = bytes.get(at + 1) else {
            return Header::NeedsMore;
        };
        match marker {
            // A fill byte: any number of 0xFF may precede a marker.
            0xFF => {
                at += 1;
                continue;
            }
            // Markers that stand alone, with no length: TEM, RSTn, SOI.
            0x01 | 0xD0..=0xD8 => {
                at += 2;
                continue;
            }
            // A stuffed zero, the end of the image, or the start of the
            // scan: there is no frame header before any of these.
            0x00 | 0xD9 | 0xDA => return Header::Unreadable,
            _ => {}
        }
        let Some(&[len_hi, len_lo]) = bytes.get(at + 2..at + 4) else {
            return Header::NeedsMore;
        };
        let len = usize::from(u16::from_be_bytes([len_hi, len_lo]));
        if is_frame_header(marker) {
            let Some(&[h0, h1, w0, w1]) = bytes.get(at + 5..at + 9) else {
                return Header::NeedsMore;
            };
            return sized(
                u32::from(u16::from_be_bytes([w0, w1])),
                u32::from(u16::from_be_bytes([h0, h1])),
            );
        }
        if len < 2 {
            return Header::Unreadable;
        }
        at += 2 + len;
    }
}

/// Whether `marker` is a start-of-frame: `SOF0` to `SOF15`, less the three
/// markers in that range that are not one (`DHT`, `JPG` and `DAC`).
const fn is_frame_header(marker: u8) -> bool {
    matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC)
}

#[cfg(test)]
#[path = "image_size_tests.rs"]
mod image_size_tests;
