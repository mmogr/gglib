//! The drawing tool's pure parts: the progress count, the token it is sent
//! under, the sentence, and the index entry.

use std::time::Duration;

use gglib_core::McpTool;
use gglib_core::ports::{GeneratedImage, ImageBatch, ImageStage};
use serde_json::json;

use super::super::meta_tools::index_of;
use super::{Meter, TOOL_ID, drew_sentence, progress_frame, progress_token};

const fn step(pass: u32, step: u32, total: u32) -> ImageStage {
    ImageStage::Sampling { pass, step, total }
}

const fn queued(position: u32) -> ImageStage {
    ImageStage::Queued {
        position,
        behind: None,
    }
}

/// Every report of a two-image render counts one, the total is known from
/// the first step, and the last count is the total.
#[test]
fn the_count_rises_by_one_a_report_and_ends_at_its_total() {
    let mut meter = Meter::new(2);
    let render = [
        queued(2),
        queued(1),
        ImageStage::Loading,
        step(1, 1, 2),
        step(1, 2, 2),
        step(2, 1, 2),
        step(2, 2, 2),
        ImageStage::Decoding,
        ImageStage::Finishing,
    ];
    let counts: Vec<_> = render.iter().map(|stage| meter.count(stage)).collect();
    assert_eq!(
        counts,
        [
            Some((1, None)),
            Some((2, None)),
            Some((3, None)),
            Some((4, Some(9))),
            Some((5, Some(9))),
            Some((6, Some(9))),
            Some((7, Some(9))),
            Some((8, Some(9))),
            Some((9, Some(9))),
        ]
    );
}

/// MCP requires the count to rise: a step reported twice, and a wait between
/// passes, send nothing.
#[test]
fn a_report_that_would_not_raise_the_count_sends_nothing() {
    let mut meter = Meter::new(2);
    assert_eq!(meter.count(&step(1, 1, 4)), Some((1, Some(10))));
    assert_eq!(meter.count(&step(1, 1, 4)), None);
    assert_eq!(meter.count(&ImageStage::Loading), None);
    assert_eq!(meter.count(&queued(1)), None);
    assert_eq!(meter.count(&step(2, 1, 4)), Some((5, Some(10))));
}

/// A render that reports no step still counts up, with no total to give.
#[test]
fn a_render_with_no_steps_counts_without_a_total() {
    let mut meter = Meter::new(1);
    assert_eq!(meter.count(&ImageStage::Loading), Some((1, None)));
    assert_eq!(meter.count(&ImageStage::Decoding), Some((2, None)));
    assert_eq!(meter.count(&ImageStage::Finishing), Some((3, None)));
}

#[test]
fn a_frame_needs_a_token_and_does_not_count_without_one() {
    let mut meter = Meter::new(1);
    assert_eq!(progress_frame(None, &mut meter, &ImageStage::Loading), None);
    let token = json!("t");
    assert_eq!(
        progress_frame(Some(&token), &mut meter, &ImageStage::Loading),
        Some(json!({
            "jsonrpc": "2.0",
            "method": "notifications/progress",
            "params": {"progressToken": "t", "progress": 1, "message": "loading the image model"},
        }))
    );
}

#[test]
fn the_token_is_a_string_or_a_number_under_meta() {
    assert_eq!(
        progress_token(&json!({"_meta": {"progressToken": "abc"}})),
        Some(json!("abc"))
    );
    assert_eq!(
        progress_token(&json!({"_meta": {"progressToken": 7}})),
        Some(json!(7))
    );
    assert_eq!(
        progress_token(&json!({"_meta": {"progressToken": null}})),
        None
    );
    assert_eq!(progress_token(&json!({"_meta": {}})), None);
    assert_eq!(progress_token(&json!({"progressToken": "abc"})), None);
}

#[test]
fn the_sentence_counts_its_images() {
    let image = GeneratedImage {
        bytes: vec![1],
        mime: "image/png",
        width: 768,
        height: 512,
    };
    let batch = ImageBatch {
        model: "flux".to_owned(),
        images: vec![image.clone(), image],
        elapsed: Duration::from_millis(75_600),
    };
    assert_eq!(
        drew_sentence(&batch),
        "Drew 2 images, 768x512 PNG, with flux in 76 s; they are attached to this result, and \
         gglib kept no copy."
    );
}

/// The gateway's id is its own: a server's tool that would take it (a server
/// stored as `builtin` before the name was kept) is left out, with drawing
/// offered or not, and the server's other tools stay.
#[test]
fn a_servers_tool_never_takes_the_gateways_id() {
    let servers = || {
        [
            (
                TOOL_ID.to_owned(),
                McpTool::new("generate_image").with_description("theirs"),
            ),
            ("builtin__other".to_owned(), McpTool::new("other")),
        ]
        .into_iter()
    };

    let without = index_of(servers(), false);
    assert!(!without.contains(TOOL_ID));
    assert!(without.contains("builtin__other"));

    let with = index_of(servers(), true);
    assert!(with.contains("builtin__other"));
    let listed = with.search("generate_image");
    assert_eq!(listed.len(), 1);
    assert!(
        listed[0].description.starts_with("Draw an image"),
        "{listed:?}"
    );
}
