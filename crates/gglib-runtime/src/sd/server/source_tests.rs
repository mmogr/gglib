//! A request's picture: what is refused before anything queues, what
//! `sd-server` is sent, and which pixels of a masked result are whose.

use std::io::Cursor;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use gglib_core::ports::{ImageError, ImageSource};
use image::{DynamicImage, GrayImage, ImageFormat, Luma, Rgb, RgbImage, Rgba, RgbaImage};

use super::{FEATHER, Source};

const SIDE: u32 = 64;
/// The mask's white: one pixel, far enough from every edge that the fade
/// around it is whole.
const WHITE: (u32, u32) = (32, 32);

fn png_of(image: &DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("a PNG");
    bytes
}

/// A picture whose every pixel differs from its neighbours and from
/// [`drawn`]'s.
fn picture() -> RgbImage {
    RgbImage::from_fn(SIDE, SIDE, |x, y| {
        Rgb([u8::try_from(x).unwrap(), u8::try_from(y).unwrap(), 7])
    })
}

/// What `sd-server` drew: no pixel of it is the picture's.
fn drawn() -> RgbImage {
    RgbImage::from_pixel(SIDE, SIDE, Rgb([250, 240, 230]))
}

fn mask_with(white: &[(u32, u32)]) -> GrayImage {
    let mut mask = GrayImage::new(SIDE, SIDE);
    for &(x, y) in white {
        mask.put_pixel(x, y, Luma([255]));
    }
    mask
}

fn source(mask: Option<&GrayImage>, strength: f32) -> ImageSource {
    ImageSource {
        image: png_of(&DynamicImage::ImageRgb8(picture())),
        mask: mask.map(|m| png_of(&DynamicImage::ImageLuma8(m.clone()))),
        strength,
    }
}

fn pasted(mask: &GrayImage) -> RgbImage {
    let source = Source::read(&source(Some(mask), 0.6)).expect("a source");
    let bytes = source
        .paste(png_of(&DynamicImage::ImageRgb8(drawn())))
        .expect("pasted");
    image::load_from_memory(&bytes).expect("a PNG").to_rgb8()
}

fn message(error: &ImageError) -> String {
    assert_eq!(error.code(), Some("invalid_request"), "{error:?}");
    error.to_string()
}

/// On the white the result is `sd-server`'s pixel; more than [`FEATHER`]
/// pixels from it, along a row, a column or both, it is the picture's own;
/// at [`FEATHER`] pixels and nearer it is neither, a blend of the two.
#[test]
fn a_masked_result_is_the_pictures_own_beyond_the_feather() {
    let pasted = pasted(&mask_with(&[WHITE]));
    let (picture, drawn) = (picture(), drawn());
    let (x, y) = WHITE;

    assert_eq!(pasted.get_pixel(x, y), drawn.get_pixel(x, y));
    for (dx, dy) in [
        (FEATHER + 1, 0),
        (0, FEATHER + 1),
        (FEATHER + 1, FEATHER + 1),
    ] {
        for (px, py) in [(x + dx, y + dy), (x - dx, y - dy)] {
            assert_eq!(
                pasted.get_pixel(px, py),
                picture.get_pixel(px, py),
                "({px}, {py}) is beyond the feather"
            );
        }
    }
    for distance in [1, FEATHER / 2, FEATHER] {
        let at = pasted.get_pixel(x + distance, y);
        assert_ne!(at, picture.get_pixel(x + distance, y), "{distance} px out");
        assert_ne!(at, drawn.get_pixel(x + distance, y), "{distance} px out");
    }
    // The fade only falls: each pixel further out is nearer the picture's.
    let red = |distance: u32| pasted.get_pixel(x + distance, y).0[0];
    assert!((0..FEATHER).all(|d| red(d) >= red(d + 1)), "the fade rises");
}

/// Every pixel of the picture more than [`FEATHER`] from the white comes
/// back byte for byte, counted over the whole picture; white at a corner
/// fades to the edge and no further.
#[test]
fn every_pixel_beyond_the_feather_is_kept_whatever_the_masks_place() {
    for white in [(0, 0), (SIDE - 1, SIDE - 1), (0, SIDE - 1), WHITE] {
        let pasted = pasted(&mask_with(&[white]));
        let picture = picture();
        for (x, y, pixel) in pasted.enumerate_pixels() {
            let far = x.abs_diff(white.0).max(y.abs_diff(white.1)) > FEATHER;
            assert_eq!(
                pixel == picture.get_pixel(x, y),
                far,
                "white at {white:?}, pixel ({x}, {y})"
            );
        }
    }
}

/// With no mask the whole picture was drawn again, and the result is handed
/// back as `sd-server` wrote it.
#[test]
fn a_result_with_no_mask_is_handed_back_as_it_came() {
    let source = Source::read(&source(None, 0.6)).expect("a source");
    // With alpha and with bytes after the PNG's end, so a result decoded and
    // written again, however faithfully, would not be these bytes.
    let translucent = RgbaImage::from_pixel(SIDE, SIDE, Rgba([250, 240, 230, 9]));
    let mut result = png_of(&DynamicImage::ImageRgba8(translucent));
    result.extend(b"trailer");
    assert_eq!(source.paste(result.clone()).expect("handed back"), result);
}

/// The fade is the 16 pixels the docs give.
#[test]
fn the_feather_is_sixteen_pixels() {
    assert_eq!(FEATHER, 16);
}

/// A result whose header is not the picture's size, or has no size to
/// read, is the runtime's failure, with or without a mask.
#[test]
fn a_result_of_another_size_is_a_failure() {
    let small = png_of(&DynamicImage::ImageRgb8(RgbImage::new(SIDE, SIDE - 16)));
    for mask in [None, Some(mask_with(&[WHITE]))] {
        let source = Source::read(&source(mask.as_ref(), 0.6)).expect("a source");
        for result in [small.clone(), b"not a png".to_vec()] {
            let failed = source.paste(result).unwrap_err();
            assert_eq!(failed.code(), Some("image_generation_failed"), "{failed:?}");
        }
    }
}

/// A mask's grey is read as black below the middle and white from it.
#[test]
fn a_masks_grey_is_black_below_the_middle_and_white_from_it() {
    let (x, y) = WHITE;
    let mut mask = GrayImage::new(SIDE, SIDE);
    mask.put_pixel(x, y, Luma([128]));
    mask.put_pixel(2, 2, Luma([127]));
    let pasted = pasted(&mask);
    assert_eq!(pasted.get_pixel(x, y), drawn().get_pixel(x, y));
    assert_eq!(pasted.get_pixel(2, 2), picture().get_pixel(2, 2));
}

/// What no render could use is refused in words that say what to change.
#[test]
fn a_source_no_render_could_use_is_refused() {
    let white = mask_with(&[WHITE]);
    let refused = |source: ImageSource| message(&Source::read(&source).unwrap_err());

    for strength in [0.0, 0.049, 1.01, -0.5, f32::NAN] {
        let words = refused(source(Some(&white), strength));
        assert!(words.contains("strength is from 0.05 to 1"), "{words}");
    }
    for strength in [0.05, 1.0] {
        Source::read(&source(Some(&white), strength)).expect("the ends are in");
    }

    let mut not_a_picture = source(None, 0.6);
    not_a_picture.image = b"not a png".to_vec();
    assert!(refused(not_a_picture).contains("the picture is not a PNG"));

    let mut not_a_mask = source(None, 0.6);
    not_a_mask.mask = Some(b"not a png".to_vec());
    assert!(refused(not_a_mask).contains("the mask is not a PNG"));

    let mut other_size = source(None, 0.6);
    other_size.mask = Some(png_of(&DynamicImage::ImageLuma8(GrayImage::new(SIDE, 32))));
    assert!(refused(other_size).contains("the mask is 64x32 and the picture 64x64"));

    let words = refused(source(Some(&GrayImage::new(SIDE, SIDE)), 0.6));
    assert!(words.contains("the mask has no white in it"), "{words}");
}

/// `sd-server` is sent the picture without its alpha and the mask as one
/// channel of black and white, whatever a client exported.
#[test]
fn the_picture_is_sent_without_alpha_and_the_mask_as_one_channel() {
    let translucent = RgbaImage::from_pixel(SIDE, SIDE, Rgba([9, 8, 7, 100]));
    let mut colour_mask = RgbImage::new(SIDE, SIDE);
    colour_mask.put_pixel(1, 1, Rgb([255, 255, 255]));
    colour_mask.put_pixel(2, 2, Rgb([200, 200, 200]));
    let source = Source::read(&ImageSource {
        image: png_of(&DynamicImage::ImageRgba8(translucent)),
        mask: Some(png_of(&DynamicImage::ImageRgb8(colour_mask))),
        strength: 0.6,
    })
    .expect("a source");

    let (picture, mask) = source.wire().expect("encoded");
    let decode = |b64: &str| image::load_from_memory(&BASE64.decode(b64).unwrap()).unwrap();
    let picture = decode(&picture);
    assert_eq!(picture.color(), image::ColorType::Rgb8);
    assert_eq!(picture.to_rgb8().get_pixel(0, 0), &Rgb([9, 8, 7]));
    let mask = decode(&mask.expect("a mask"));
    assert_eq!(mask.color(), image::ColorType::L8);
    let mask = mask.to_luma8();
    assert_eq!(mask.get_pixel(1, 1), &Luma([255]));
    assert_eq!(mask.get_pixel(2, 2), &Luma([255]), "grey above the middle");
    assert_eq!(mask.get_pixel(0, 0), &Luma([0]));

    let (_, none) = Source::read(&self::source(None, 0.6))
        .unwrap()
        .wire()
        .unwrap();
    assert_eq!(none, None, "no mask is sent for a whole redraw");
}
