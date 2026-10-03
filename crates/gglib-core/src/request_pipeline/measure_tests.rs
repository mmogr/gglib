//! Tests for [`super::measured_len`], and for what
//! [`truncate_history`](super::super::truncation::truncate_history) does to
//! a request with images once it measures them this way.

use serde_json::json;

use super::super::image_fixtures::png_url_of;
use super::super::images::MAX_IMAGE_TOKENS;
use super::super::tests_support::chars;
use super::super::truncation::{
    PROTECTED_TAIL_COUNT, TRUNCATION_PLACEHOLDER, TruncationError, truncate_history,
};
use super::*;

/// A 32,768-token context at the static ratio.
const CONTEXT_32K: ContextBudget = ContextBudget {
    chars: 131_072,
    tokens: 32_768,
};

/// A 2560x1440 screenshot as a data URL about 1.5 MB long: 3,600 tokens.
fn screenshot() -> String {
    let url = png_url_of(2560, 1440, 1_125_000);
    assert!(url.len() > 1_500_000);
    url
}

fn image_part(url: &str) -> Value {
    json!({"type": "image_url", "image_url": {"url": url}})
}

fn user_with(text: &str, urls: &[&str]) -> Value {
    let mut parts = vec![json!({"type": "text", "text": text})];
    parts.extend(urls.iter().map(|url| image_part(url)));
    json!({"role": "user", "content": parts})
}

fn body(messages: &[Value]) -> Value {
    json!({"model": "m", "messages": messages})
}

fn wire_len(body: &Value) -> usize {
    serde_json::to_string(body)
        .expect("a value serializes")
        .len()
}

#[test]
fn a_request_with_no_images_measures_its_serialized_length() {
    let body = body(&[
        json!({"role": "system", "content": "be brief"}),
        user_with("a \"quoted\" line\nand a second", &[]),
    ]);
    assert_eq!(measured_len(&body, CONTEXT_32K), wire_len(&body));
}

#[test]
fn an_image_is_counted_at_its_tokens_in_place_of_its_url() {
    let url = screenshot();
    let body = body(&[user_with("what is the error?", &[&url])]);
    let without_url = wire_len(&body) - url.len();

    assert_eq!(measured_len(&body, CONTEXT_32K), without_url + 3600 * 4);
    // The same image under a learned ratio of 3.3 characters a token.
    let learned = ContextBudget {
        chars: 99_000,
        tokens: 30_000,
    };
    assert_eq!(measured_len(&body, learned), without_url + 11_880);
}

#[test]
fn an_image_whose_size_cannot_be_read_is_counted_at_the_cap() {
    let url = "https://example.com/cat.png";
    let body = body(&[user_with("what is this?", &[url])]);
    assert_eq!(
        measured_len(&body, CONTEXT_32K),
        wire_len(&body) - url.len() + MAX_IMAGE_TOKENS * 4
    );
}

#[test]
fn a_fraction_of_a_character_rounds_up() {
    // 1 token at 10 characters to 3 tokens.
    let budget = ContextBudget {
        chars: 10,
        tokens: 3,
    };
    assert_eq!(budget.chars_for(1), 4);
    assert_eq!(budget.chars_for(3), 10);
    let no_context = ContextBudget {
        chars: 10,
        tokens: 0,
    };
    assert_eq!(no_context.chars_for(5), 0);
}

/// A 1.5 MB screenshot is twelve times a 32k context by its length and a
/// ninth of it by its tokens. It is forwarded whole.
#[test]
fn a_screenshot_under_a_32k_context_is_kept_whole_and_not_refused() {
    let url = screenshot();
    let mut request = body(&[user_with("what is the error?", &[&url])]);
    let before = request.clone();
    assert!(wire_len(&request) > CONTEXT_32K.chars * 10);

    let report = truncate_history(&mut request, CONTEXT_32K).expect("it fits by its tokens");

    assert_eq!(request, before, "nothing is trimmed");
    assert_eq!(report.messages_truncated, 0);
    assert!(report.payload_chars_before < 20_000, "{report:?}");
}

/// The control: the same characters as text are what they look like, and
/// are refused.
#[test]
fn the_same_characters_as_text_are_refused() {
    let url = screenshot();
    let mut request = body(&[user_with(&url, &[])]);

    let err = truncate_history(&mut request, CONTEXT_32K).unwrap_err();

    let TruncationError::ExceedsBudgetAfterTruncation { payload_chars, .. } = err;
    assert!(payload_chars > 1_500_000);
}

#[test]
fn images_past_the_budget_are_refused_as_any_request_past_it_is() {
    // Nine images at the cap are 36,864 tokens, over the 32,768 there are.
    let urls = ["https://example.com/cat.png"; 9];
    let mut request = body(&[user_with("compare these", &urls)]);

    let err = truncate_history(&mut request, CONTEXT_32K).unwrap_err();

    assert_eq!(
        err,
        TruncationError::ExceedsBudgetAfterTruncation {
            payload_chars: measured_len(&request, CONTEXT_32K),
            limit_chars: CONTEXT_32K.chars,
        }
    );
    // Seven are under it.
    let mut fits = body(&[user_with("compare these", &urls[..7])]);
    assert!(truncate_history(&mut fits, CONTEXT_32K).is_ok());
}

/// Text is elided and the images alone are still past the budget: what is
/// left is measured with its images, and refused.
#[test]
fn images_still_past_the_budget_after_trimming_are_refused() {
    let urls = ["https://example.com/cat.png"; 9];
    let mut messages = vec![
        user_with("compare these", &urls),
        json!({"role": "tool", "tool_call_id": "c1", "content": "x".repeat(60_000)}),
    ];
    messages.extend((0..PROTECTED_TAIL_COUNT).map(|_| json!({"role": "user", "content": "ok"})));
    let mut request = body(&messages);

    let err = truncate_history(&mut request, CONTEXT_32K).unwrap_err();

    assert_eq!(request["messages"][1]["content"], TRUNCATION_PLACEHOLDER);
    assert!(wire_len(&request) < CONTEXT_32K.chars, "the text fits");
    assert_eq!(
        err,
        TruncationError::ExceedsBudgetAfterTruncation {
            payload_chars: measured_len(&request, CONTEXT_32K),
            limit_chars: CONTEXT_32K.chars,
        }
    );
}

/// Trimming rewrites a message's text and nothing else: an image beside the
/// elided text, and one in a message that is not touched, are the bytes the
/// client sent.
#[test]
fn an_image_in_a_message_that_is_kept_survives_trimming_byte_for_byte() {
    let url = screenshot();
    let small = png_url_of(640, 480, 4_000);
    let mut messages = vec![
        user_with("first, this screenshot", &[&url]),
        json!({"role": "tool", "tool_call_id": "c1", "content": [
            {"type": "text", "text": "x".repeat(60_000)},
            image_part(&small),
        ]}),
    ];
    messages.extend((0..PROTECTED_TAIL_COUNT).map(|_| json!({"role": "user", "content": "ok"})));
    let mut request = body(&messages);
    let before = request.clone();
    let image_bytes = |request: &Value, message: usize, part: usize| {
        serde_json::to_vec(&request["messages"][message]["content"][part]).expect("serializes")
    };

    let report = truncate_history(&mut request, chars(70_000)).expect("trimmed to fit");

    assert_eq!(report.messages_truncated, 1);
    assert_eq!(
        request["messages"][1]["content"][0]["text"],
        TRUNCATION_PLACEHOLDER
    );
    assert_eq!(image_bytes(&request, 1, 1), image_bytes(&before, 1, 1));
    assert_eq!(request["messages"][0], before["messages"][0]);
    assert_eq!(image_bytes(&request, 0, 1), image_bytes(&before, 0, 1));
    assert_eq!(
        request["messages"][1]["content"][1]["image_url"]["url"],
        small
    );
}
