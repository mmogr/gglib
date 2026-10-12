//! A request that draws from a picture, against the scripted job of
//! `job_tests.rs`: what `sd-server` is sent, what is refused before the
//! model is admitted, and what a masked render is answered with.

use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use gglib_core::ports::{ImageError, ImageGenerationPort, ImageRequest, ImageSize, ImageSource};
use image::{DynamicImage, GrayImage, ImageFormat, Luma, Rgb, RgbImage};
use tokio::sync::mpsc;

use super::{Host, Timeline, drain, driver, models, sdxl_request};
use crate::sd::server::job_api::{JobState, SampleParams};
use crate::sd::server::job_plan::steps_for;
use crate::sd::server::source::FEATHER;

/// A size SDXL draws, and not its default square.
const SIZE: (u32, u32) = (256, 320);
const PICTURE: Rgb<u8> = Rgb([10, 20, 30]);
const DRAWN: Rgb<u8> = Rgb([250, 240, 230]);
/// The one white pixel of [`mask`].
const WHITE: (u32, u32) = (100, 100);
/// SDXL's recipe.
const SDXL_STEPS: u32 = 20;

fn png_of(image: &DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("a PNG");
    bytes
}

fn solid(size: (u32, u32), colour: Rgb<u8>) -> Vec<u8> {
    png_of(&DynamicImage::ImageRgb8(RgbImage::from_pixel(
        size.0, size.1, colour,
    )))
}

fn mask(size: (u32, u32), white: &[(u32, u32)]) -> Vec<u8> {
    let mut mask = GrayImage::new(size.0, size.1);
    for &(x, y) in white {
        mask.put_pixel(x, y, Luma([255]));
    }
    png_of(&DynamicImage::ImageLuma8(mask))
}

fn from_picture(mask: Option<Vec<u8>>, strength: f32) -> ImageRequest {
    ImageRequest {
        source: Some(ImageSource {
            image: solid(SIZE, PICTURE),
            mask,
            strength,
        }),
        ..sdxl_request()
    }
}

/// A job that finishes with `sd-server`'s picture, [`DRAWN`] all over.
fn drawing() -> Arc<Timeline> {
    let result = BASE64.encode(solid(SIZE, DRAWN));
    Arc::new(Timeline::new(vec![(0, JobState::Completed(vec![result]))]))
}

fn decoded(b64: &str) -> DynamicImage {
    image::load_from_memory(&BASE64.decode(b64).expect("base64")).expect("a PNG")
}

/// A masked request is sent at the picture's size with the picture, the
/// mask, the strength and the recipe's steps over the strength; its answer
/// is the picture with `sd-server`'s pixels on the white alone.
#[tokio::test(start_paused = true)]
async fn a_masked_request_sends_its_picture_and_is_answered_with_the_patch_pasted() {
    let jobs = drawing();
    let host = Host::new();
    let driver = driver(models(), &jobs, &host);
    let (tx, _rx) = mpsc::channel(64);

    let request = from_picture(Some(mask(SIZE, &[WHITE])), 0.5);
    let batch = driver.generate(request, tx).await.expect("drawn");

    let body = jobs.bodies.lock().unwrap()[0].clone();
    assert_eq!((body.width, body.height), SIZE, "the picture's size");
    assert_eq!(body.strength, Some(0.5));
    assert_eq!(
        body.sample_params,
        Some(SampleParams {
            sample_steps: SDXL_STEPS * 2
        })
    );
    let sent = decoded(body.init_image.as_deref().expect("the picture is sent")).to_rgb8();
    assert_eq!((sent.dimensions(), sent.get_pixel(0, 0)), (SIZE, &PICTURE));
    let sent = decoded(body.mask_image.as_deref().expect("the mask is sent")).to_luma8();
    assert_eq!(sent.get_pixel(WHITE.0, WHITE.1), &Luma([255]));
    assert_eq!(sent.get_pixel(0, 0), &Luma([0]));

    let image = &batch.images[0];
    assert_eq!(
        (image.width, image.height, image.mime),
        (256, 320, "image/png")
    );
    let answer = image::load_from_memory(&image.bytes)
        .expect("a PNG")
        .to_rgb8();
    assert_eq!(answer.get_pixel(WHITE.0, WHITE.1), &DRAWN);
    assert_eq!(answer.get_pixel(WHITE.0 + FEATHER + 1, WHITE.1), &PICTURE);
    assert_eq!(answer.get_pixel(0, 0), &PICTURE);
}

/// With no mask the picture, the strength and the steps are sent, no mask
/// is, and the answer is `sd-server`'s picture as it wrote it.
#[tokio::test(start_paused = true)]
async fn a_whole_redraw_sends_no_mask_and_is_answered_as_drawn() {
    let jobs = drawing();
    let host = Host::new();
    let driver = driver(models(), &jobs, &host);
    let (tx, _rx) = mpsc::channel(64);

    let batch = driver
        .generate(from_picture(None, 0.8), tx)
        .await
        .expect("drawn");

    let body = jobs.bodies.lock().unwrap()[0].clone();
    assert!(body.init_image.is_some());
    assert_eq!(body.mask_image, None);
    assert_eq!(body.strength, Some(0.8));
    assert_eq!(
        body.sample_params.map(|p| p.sample_steps),
        Some(steps_for(SDXL_STEPS, 0.8))
    );
    assert_eq!(batch.images[0].bytes, solid(SIZE, DRAWN));
}

/// A request with no picture sends none of a redraw's fields.
#[tokio::test(start_paused = true)]
async fn a_request_with_no_picture_sends_none_of_it() {
    let result = BASE64.encode(solid((1024, 1024), DRAWN));
    let jobs = Arc::new(Timeline::new(vec![(0, JobState::Completed(vec![result]))]));
    let host = Host::new();
    let driver = driver(models(), &jobs, &host);
    let (tx, _rx) = mpsc::channel(64);

    driver.generate(sdxl_request(), tx).await.expect("drawn");

    let body = jobs.bodies.lock().unwrap()[0].clone();
    assert_eq!(
        (
            body.init_image,
            body.mask_image,
            body.strength,
            body.sample_params
        ),
        (None, None, None, None)
    );
}

/// A picture the model's family does not draw at, a size beside a picture,
/// a mask of another size and a strength out of range are refused before
/// the model is admitted and before any stage is reported.
#[tokio::test(start_paused = true)]
async fn a_picture_the_request_cannot_be_drawn_from_never_queues() {
    let jobs = drawing();
    let host = Host::new();
    let driver = driver(models(), &jobs, &host);
    let (tx, mut rx) = mpsc::channel(64);
    let with = |source: ImageSource| ImageRequest {
        source: Some(source),
        ..sdxl_request()
    };
    let refused = |error: ImageError| {
        assert_eq!(error.code(), Some("invalid_request"), "{error:?}");
        error.to_string()
    };

    // 300 is not a multiple of SDXL's 64.
    let off_rule = with(ImageSource {
        image: solid((256, 300), PICTURE),
        mask: None,
        strength: 0.6,
    });
    let words = refused(driver.generate(off_rule, tx.clone()).await.unwrap_err());
    assert!(
        words.contains("the picture is 256x300, which sdxl does not draw"),
        "{words}"
    );

    let sized = ImageRequest {
        size: Some(ImageSize {
            width: 256,
            height: 320,
        }),
        ..from_picture(None, 0.6)
    };
    let words = refused(driver.generate(sized, tx.clone()).await.unwrap_err());
    assert!(words.contains("leave size out"), "{words}");

    let other_mask = from_picture(Some(mask((256, 256), &[(1, 1)])), 0.6);
    let words = refused(driver.generate(other_mask, tx.clone()).await.unwrap_err());
    assert!(words.contains("a mask is the picture's size"), "{words}");

    let too_strong = from_picture(None, 1.5);
    let words = refused(driver.generate(too_strong, tx).await.unwrap_err());
    assert!(words.contains("strength is from"), "{words}");

    assert!(drain(&mut rx).is_empty(), "no stage was reported");
    assert_eq!(
        host.admits.load(Ordering::SeqCst),
        0,
        "nothing was admitted"
    );
    assert!(
        jobs.bodies.lock().unwrap().is_empty(),
        "nothing was submitted"
    );
}

/// The steps asked for are the fewest whose share at `strength` is the
/// recipe's count, and never fewer than the recipe's.
#[test]
fn the_steps_asked_for_leave_the_recipes_count_after_the_strength() {
    for (recipe, strength, steps) in [
        (4, 1.0, 4),
        (4, 0.5, 8),
        (4, 0.6, 7),
        (8, 0.75, 11),
        (20, 0.75, 27),
        (20, 0.05, 400),
    ] {
        assert_eq!(steps_for(recipe, strength), steps, "{recipe} at {strength}");
    }
    for no_strength in [0.0, -1.0, f32::NAN] {
        assert_eq!(steps_for(8, no_strength), 8, "{no_strength}");
    }
    for recipe in [4_u32, 8, 20] {
        for strength in [0.05_f32, 0.3, 0.6, 0.85, 1.0] {
            let steps = steps_for(recipe, strength);
            let share = |steps: u32| f64::from(steps) * f64::from(strength);
            assert!(share(steps) >= f64::from(recipe), "{recipe} at {strength}");
            assert!(
                share(steps - 1) < f64::from(recipe),
                "{recipe} at {strength}"
            );
        }
    }
}
