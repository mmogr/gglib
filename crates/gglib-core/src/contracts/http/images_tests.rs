//! The drawing route's wire, pinned as bytes.

use serde_json::json;

use super::*;

/// The event names are `OpenAI`'s, checked against its SDKs, and gglib's own.
#[test]
fn the_event_names_are_the_ones_the_sdks_read() {
    assert_eq!(PARTIAL_IMAGE_EVENT, "image_generation.partial_image");
    assert_eq!(COMPLETED_EVENT, "image_generation.completed");
    assert_eq!(PROGRESS_EVENT, "image_generation.progress");
    let completed = ImageStreamEvent::Completed {
        b64_json: "AAA".into(),
        size: "1024x1024".into(),
        output_format: "png".into(),
        created_at: 7,
        model: Some("flux1-schnell".into()),
    };
    assert_eq!(
        serde_json::to_value(&completed).unwrap(),
        json!({"type": COMPLETED_EVENT, "b64_json": "AAA", "size": "1024x1024",
               "output_format": "png", "created_at": 7, "model": "flux1-schnell"})
    );
    let partial = ImageStreamEvent::PartialImage {
        b64_json: "BB".into(),
        partial_image_index: 0,
        size: "1024x1024".into(),
        output_format: "png".into(),
        created_at: 7,
    };
    assert_eq!(
        serde_json::to_value(&partial).unwrap()["type"],
        json!(PARTIAL_IMAGE_EVENT)
    );
}

/// A completed event from a daemon that names no model still reads, with
/// none.
#[test]
fn a_completed_event_without_a_model_reads_with_none() {
    let old: ImageStreamEvent = serde_json::from_value(json!({
        "type": COMPLETED_EVENT, "b64_json": "AAA", "size": "8x8",
        "output_format": "png", "created_at": 7
    }))
    .unwrap();
    assert!(matches!(
        old,
        ImageStreamEvent::Completed { model: None, .. }
    ));
}

/// A progress event writes only what its stage has, and reads back whole.
#[test]
fn a_progress_event_carries_only_what_its_stage_has() {
    let loading = ImageStreamEvent::Progress {
        stage: "loading".into(),
        pass: None,
        step: None,
        total: None,
        position: None,
        behind: None,
        frame_b64: None,
    };
    assert_eq!(
        serde_json::to_string(&loading).unwrap(),
        r#"{"type":"image_generation.progress","stage":"loading"}"#
    );
    let sampling = ImageStreamEvent::Progress {
        stage: "sampling".into(),
        pass: Some(1),
        step: Some(2),
        total: Some(4),
        position: None,
        behind: None,
        frame_b64: Some("CC".into()),
    };
    let text = serde_json::to_string(&sampling).unwrap();
    assert_eq!(
        serde_json::from_str::<ImageStreamEvent>(&text).unwrap(),
        sampling
    );
}

/// A request reads with gglib's seed and ignores fields it does not know.
#[test]
fn a_request_ignores_fields_it_does_not_know() {
    let request: ImageGenerationsRequest = serde_json::from_value(json!({
        "model": "flux", "prompt": "a cat", "n": 2, "size": "1024x768",
        "seed": 42, "quality": "high", "background": "auto", "stream": true,
        "partial_images": 2
    }))
    .unwrap();
    assert_eq!(
        request,
        ImageGenerationsRequest {
            model: Some("flux".into()),
            prompt: "a cat".into(),
            n: Some(2),
            size: Some("1024x768".into()),
            seed: Some(42),
            response_format: None,
            output_format: None,
            stream: true,
            partial_images: Some(2),
        }
    );
    assert_eq!(
        serde_json::to_string(&ImageGenerationsRequest {
            prompt: "a cat".into(),
            ..ImageGenerationsRequest::default()
        })
        .unwrap(),
        r#"{"prompt":"a cat"}"#
    );
}

/// Partial images come at most as often as asked, evenly spread, the last
/// at the last step; none when none are asked for.
#[test]
fn partial_images_are_spread_evenly_and_capped() {
    assert_eq!(partial_steps(4, 0), Vec::<u32>::new());
    assert_eq!(partial_steps(4, 1), [4]);
    assert_eq!(partial_steps(4, 2), [2, 4]);
    assert_eq!(partial_steps(4, 3), [2, 3, 4]);
    assert_eq!(partial_steps(20, 3), [7, 14, 20]);
    assert_eq!(partial_steps(2, 3), [1, 2]);
    assert_eq!(partial_steps(0, 3), Vec::<u32>::new());
    for total in 1..=40 {
        for wanted in 0..=MAX_PARTIAL_IMAGES {
            let steps = partial_steps(total, wanted);
            assert!(steps.len() <= wanted as usize, "{total} {wanted}");
            assert!(steps.iter().all(|s| (1..=total).contains(s)));
        }
    }
}
