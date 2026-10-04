//! Tests for [`super`]: real headers, built in
//! [`image_fixtures`](super::super::image_fixtures), and input that is cut
//! short or is not an image at all.

use super::super::image_fixtures::{data_url, frame, jpeg, jpeg_with, png, segment};
use super::*;

/// An `APP1` of the largest size a segment can be, as a camera's EXIF is.
fn large_app1() -> Vec<u8> {
    segment(0xE1, &vec![0x45; 65_533])
}

#[test]
fn a_png_is_its_ihdr() {
    assert_eq!(image_size(&png(2560, 1440)), Some((2560, 1440)));
    assert_eq!(image_size(&png(1, 70_000)), Some((1, 70_000)));
}

#[test]
fn a_baseline_jpeg_is_its_frame_header() {
    assert_eq!(image_size(&jpeg(980, 460)), Some((980, 460)));
}

#[test]
fn a_progressive_jpeg_is_read_from_sof2() {
    assert_eq!(
        image_size(&jpeg_with(0xC2, 4032, 3024, &[])),
        Some((4032, 3024))
    );
}

#[test]
fn every_frame_marker_is_read_and_the_three_that_are_not_one_are_skipped() {
    for sof in 0xC0..=0xCF_u8 {
        if matches!(sof, 0xC4 | 0xC8 | 0xCC) {
            // A segment under one of these, laid out like a frame header of
            // another size, comes before the real one and is passed over.
            let decoy = frame(sof, 7, 9);
            assert_eq!(
                image_size(&jpeg_with(0xC0, 640, 480, &decoy)),
                Some((640, 480)),
                "marker {sof:#04X} is not a frame header"
            );
        } else {
            assert_eq!(
                image_size(&jpeg_with(sof, 640, 480, &[])),
                Some((640, 480)),
                "marker {sof:#04X} is a frame header"
            );
        }
    }
}

#[test]
fn a_large_app1_before_the_frame_header_is_skipped() {
    let bytes = jpeg_with(0xC0, 3000, 2000, &large_app1());
    assert_eq!(image_size(&bytes), Some((3000, 2000)));
}

#[test]
fn fill_bytes_and_standalone_markers_before_a_segment_are_passed_over() {
    // Three fill bytes, then a restart marker, which has no length.
    let before = [0xFF, 0xFF, 0xFF, 0xFF, 0xD0];
    assert_eq!(
        image_size(&jpeg_with(0xC0, 800, 600, &before)),
        Some((800, 600))
    );
}

#[test]
fn every_prefix_of_an_image_is_read_without_a_panic() {
    let images = [
        (png(2560, 1440), (2560, 1440)),
        (jpeg(980, 460), (980, 460)),
        (jpeg_with(0xC2, 4032, 3024, &[]), (4032, 3024)),
        (
            jpeg_with(0xC0, 800, 600, &segment(0xE1, &[0x45; 300])),
            (800, 600),
        ),
    ];
    for (bytes, size) in images {
        let mut first_read = None;
        for len in 0..=bytes.len() {
            match image_size(&bytes[..len]) {
                None => assert_eq!(first_read, None, "a longer prefix lost the size at {len}"),
                Some(read) => {
                    assert_eq!(read, size, "a wrong size at {len}");
                    first_read.get_or_insert(len);
                }
            }
        }
        assert!(
            first_read.is_some_and(|len| len > 2),
            "never read: {size:?}"
        );
    }
}

#[test]
fn every_prefix_of_a_data_url_is_read_without_a_panic() {
    for bytes in [png(64, 48), jpeg(64, 48)] {
        let url = data_url("image/png", &bytes);
        for len in 0..=url.len() {
            let read = data_url_image_size(&url[..len]);
            assert!(
                read.is_none() || read == Some((64, 48)),
                "at {len}: {read:?}"
            );
        }
        assert_eq!(data_url_image_size(&url), Some((64, 48)));
    }
}

#[test]
fn what_is_not_a_png_or_a_jpeg_has_no_size() {
    let scan_first = [
        &[0xFF, 0xD8][..],
        &segment(0xDA, &[0; 10]),
        &frame(0xC0, 5, 5),
    ]
    .concat();
    let ended = [&[0xFF, 0xD8, 0xFF, 0xD9][..], &frame(0xC0, 5, 5)].concat();
    let no_marker = [&[0xFF, 0xD8, 0x12, 0x34][..], &frame(0xC0, 5, 5)].concat();
    let short_length = [
        &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x01][..],
        &frame(0xC0, 5, 5),
    ]
    .concat();
    let far_length = [0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF, 1, 2, 3];
    let not_ihdr = {
        let mut bytes = png(5, 5);
        bytes[12..16].copy_from_slice(b"IDAT");
        bytes
    };
    let garbage: Vec<u8> = (0..4096_u32)
        .map(|i| i.wrapping_mul(2_654_435_761).to_be_bytes()[0])
        .collect();
    let cases: [(&str, &[u8]); 12] = [
        ("nothing", &[]),
        ("a GIF", b"GIF89a\x10\x00\x10\x00"),
        ("text", b"<svg width=\"10\" height=\"10\"/>"),
        ("garbage", &garbage),
        ("all 0xFF", &[0xFF; 64]),
        ("a scan before any frame header", &scan_first),
        ("an end of image before any frame header", &ended),
        ("a byte that starts no segment", &no_marker),
        ("a segment length under two", &short_length),
        ("a segment that runs past the end", &far_length),
        ("a PNG whose first chunk is not IHDR", &not_ihdr),
        ("a PNG of no width", &png(0, 5)),
    ];
    for (what, bytes) in cases {
        assert_eq!(image_size(bytes), None, "{what}");
    }
    assert_eq!(image_size(&jpeg(0, 5)), None, "a JPEG of no width");
    assert_eq!(image_size(&jpeg(5, 0)), None, "a JPEG of no height");
}

#[test]
fn a_data_url_gives_the_size_of_the_image_it_carries() {
    assert_eq!(
        data_url_image_size(&data_url("image/png", &png(2560, 1440))),
        Some((2560, 1440))
    );
    assert_eq!(
        data_url_image_size(&data_url("image/jpeg", &jpeg(980, 460))),
        Some((980, 460))
    );
    // Sent without its trailing `=`, as some clients do.
    let unpadded = data_url("image/png", &png(31, 17)[..25]);
    assert!(unpadded.ends_with('='), "the fixture is padded");
    assert_eq!(
        data_url_image_size(unpadded.trim_end_matches('=')),
        Some((31, 17))
    );
}

#[test]
fn a_url_that_is_not_a_base64_data_url_has_no_size() {
    for url in [
        "https://example.com/cat.png",
        "http://example.com/cat.jpg",
        "data:image/png,not-base64",
        "data:image/png;base64",
        "data:image/png;base64,",
        "data:image/png;base64,!!!!",
        "",
    ] {
        assert_eq!(data_url_image_size(url), None, "{url:?}");
    }
}

/// The frame header is past the first prefix, so the prefix has to grow to
/// reach it.
#[test]
fn a_data_url_jpeg_with_a_large_app1_is_read_by_growing_the_prefix() {
    let bytes = jpeg_with(0xC0, 3000, 2000, &large_app1());
    assert!(bytes.len() > 16 * FIRST_PREFIX_BYTES);
    assert_eq!(
        data_url_image_size(&data_url("image/jpeg", &bytes)),
        Some((3000, 2000))
    );
}

/// What follows the prefix is never decoded: here it is not base64 at all,
/// and the size is read all the same.
#[test]
fn only_the_prefix_of_a_payload_is_decoded() {
    let mut bytes = png(1920, 1080);
    bytes.resize(FIRST_PREFIX_BYTES / 3 * 3, 0);
    let mut url = data_url("image/png", &bytes);
    assert!(!url.ends_with('='), "the valid part is whole groups");
    url.push_str(&"!".repeat(2_000_000));
    assert_eq!(data_url_image_size(&url), Some((1920, 1080)));
}

/// The prefix stops growing at [`MAX_HEADER_BYTES`]: a frame header past it
/// is in the bytes, and is not looked for.
#[test]
fn a_frame_header_past_the_bound_is_not_looked_for() {
    assert_eq!(MAX_HEADER_BYTES, 1 << 20);
    let metadata = large_app1().repeat(MAX_HEADER_BYTES / 65_535 + 1);
    let bytes = jpeg_with(0xC0, 3000, 2000, &metadata);
    assert_eq!(image_size(&bytes), Some((3000, 2000)), "it is there");
    assert_eq!(data_url_image_size(&data_url("image/jpeg", &bytes)), None);

    // One segment fewer and the header is inside the bound.
    let metadata = large_app1().repeat(MAX_HEADER_BYTES / 65_535 - 1);
    let bytes = jpeg_with(0xC0, 3000, 2000, &metadata);
    assert_eq!(
        data_url_image_size(&data_url("image/jpeg", &bytes)),
        Some((3000, 2000))
    );
}
