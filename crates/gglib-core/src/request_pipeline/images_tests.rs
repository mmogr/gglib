//! Tests for [`super`]: the estimate, the price of a URL, the walk over a
//! request, and the refusal.

use serde_json::json;

use super::super::image_fixtures::{data_url, jpeg, png};
use super::*;

fn image_part(url: &str) -> Value {
    json!({"type": "image_url", "image_url": {"url": url}})
}

#[test]
fn the_estimate_is_one_token_a_square_it_touches() {
    // The 2026-10-03 reading: 3,646 measured.
    assert_eq!(estimate_image_tokens(2560, 1440), 3600);
    // 31 across by 15 down; 480 measured.
    assert_eq!(estimate_image_tokens(980, 460), 465);
    assert_eq!(estimate_image_tokens(32, 32), 1);
    assert_eq!(estimate_image_tokens(33, 32), 2);
    assert_eq!(estimate_image_tokens(32, 33), 2);
}

#[test]
fn the_estimate_is_at_least_one_and_at_most_the_cap() {
    assert_eq!(estimate_image_tokens(1, 1), 1);
    assert_eq!(estimate_image_tokens(0, 0), 1);
    assert_eq!(estimate_image_tokens(8000, 8000), MAX_IMAGE_TOKENS);
    assert_eq!(estimate_image_tokens(u32::MAX, u32::MAX), MAX_IMAGE_TOKENS);
    assert_eq!(MAX_IMAGE_TOKENS, 4096);
    // The last size under the cap, and the first one over it.
    assert_eq!(estimate_image_tokens(64 * 32, 64 * 32), 4096);
    assert_eq!(estimate_image_tokens(64 * 32, 63 * 32), 4032);
}

#[test]
fn a_url_is_charged_its_estimate_when_its_size_can_be_read() {
    assert_eq!(
        image_url_tokens(&data_url("image/png", &png(2560, 1440))),
        3600
    );
    assert_eq!(
        image_url_tokens(&data_url("image/jpeg", &jpeg(980, 460))),
        465
    );
    assert_eq!(image_url_tokens(&data_url("image/png", &png(1, 1))), 1);
}

#[test]
fn a_url_whose_size_cannot_be_read_is_charged_the_cap() {
    let cut_short = data_url("image/png", &png(64, 64)[..20]);
    for url in [
        "https://example.com/cat.png",
        "data:image/gif;base64,R0lGODlhEAAQAAAAACwAAAAAEAAQAAAC",
        cut_short.as_str(),
        "",
    ] {
        assert_eq!(image_url_tokens(url), MAX_IMAGE_TOKENS, "{url:?}");
    }
}

#[test]
fn a_requests_images_are_every_messages_in_order_history_included() {
    let body = json!({"messages": [
        {"role": "system", "content": "be brief"},
        {"role": "user", "content": [{"type": "text", "text": "this?"}, image_part("u1")]},
        {"role": "assistant", "content": "a cat"},
        {"role": "tool", "content": [image_part("u2"), image_part("u3")]},
        {"role": "user", "content": "and now?"},
    ]});
    let urls: Vec<&str> = request_image_urls(&body).collect();
    assert_eq!(urls, ["u1", "u2", "u3"]);
    assert!(has_images(&body));
}

#[test]
fn a_request_with_no_image_part_has_no_images() {
    for body in [
        json!({"messages": [{"role": "user", "content": "image_url"}]}),
        json!({"messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]}),
        json!({"messages": [{"role": "assistant", "content": null}]}),
        json!({"messages": "image_url"}),
        json!({"model": "m"}),
        json!([]),
    ] {
        assert!(!has_images(&body), "{body}");
    }
}

#[test]
fn only_an_image_for_a_model_that_cannot_see_is_refused() {
    assert_eq!(refuse_unless_can_see(false, true), Err(CannotReadImages));
    assert_eq!(refuse_unless_can_see(true, true), Ok(()));
    assert_eq!(refuse_unless_can_see(false, false), Ok(()));
    assert_eq!(refuse_unless_can_see(true, false), Ok(()));
}

#[test]
fn the_refusal_names_the_model_the_cause_and_the_command() {
    assert_eq!(CannotReadImages.code(), "model_cannot_read_images");
    let message = CannotReadImages.message("qwen3-27b");
    assert!(
        message.contains("'qwen3-27b' cannot read images"),
        "{message}"
    );
    assert!(message.contains("no projector linked"), "{message}");
    assert!(
        message.contains("gglib model update qwen3-27b --projector <path>"),
        "{message}"
    );
}
