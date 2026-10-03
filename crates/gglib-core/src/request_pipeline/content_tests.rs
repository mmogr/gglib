//! Tests for [`super`]: both shapes, and the shapes that are neither.

use super::*;
use serde_json::json;

fn upper(text: &mut String) {
    *text = text.to_uppercase();
}

#[test]
fn a_string_is_its_own_length_and_is_visited_once() {
    let mut content = json!("hello");
    assert_eq!(text_len(&content), 5);
    assert_eq!(text_parts(&content), 1);
    assert_eq!(for_each_text_mut(&mut content, &mut upper), 1);
    assert_eq!(content, json!("HELLO"));
}

#[test]
fn an_array_counts_and_visits_each_text_part() {
    let mut content = json!([
        {"type": "text", "text": "ab"},
        {"type": "text", "text": "cde"},
    ]);
    assert_eq!(text_len(&content), 5);
    assert_eq!(text_parts(&content), 2);
    assert_eq!(for_each_text_mut(&mut content, &mut upper), 2);
    assert_eq!(
        content,
        json!([{"type": "text", "text": "AB"}, {"type": "text", "text": "CDE"}])
    );
}

#[test]
fn a_part_that_is_not_text_is_neither_counted_nor_touched() {
    let image = json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}});
    let mut content = json!([{"type": "text", "text": "look"}, image]);
    assert_eq!(text_len(&content), 4);
    assert_eq!(text_parts(&content), 1);
    assert_eq!(for_each_text_mut(&mut content, &mut upper), 1);
    assert_eq!(content[0], json!({"type": "text", "text": "LOOK"}));
    assert_eq!(content[1], image, "the shape and the other part are kept");
}

#[test]
fn an_array_with_no_text_part_carries_no_text() {
    let mut content = json!([{"type": "image_url", "image_url": {"url": "x"}}]);
    let before = content.clone();
    assert_eq!(text_len(&content), 0);
    assert_eq!(text_parts(&content), 0);
    assert_eq!(for_each_text_mut(&mut content, &mut upper), 0);
    assert_eq!(content, before);
}

#[test]
fn a_shape_that_is_neither_is_left_alone() {
    for mut content in [json!(null), json!(42), json!({"text": "not a part"})] {
        let before = content.clone();
        assert_eq!(text_len(&content), 0);
        assert_eq!(text_parts(&content), 0);
        assert_eq!(for_each_text_mut(&mut content, &mut upper), 0);
        assert_eq!(content, before);
    }
}

#[test]
fn a_string_carries_no_image() {
    let content = json!("see https://example.com/cat.png, an image_url");
    assert_eq!(image_urls(&content).count(), 0);
}

#[test]
fn an_arrays_image_parts_are_read_in_order() {
    let content = json!([
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
        {"type": "image_url", "image_url": {"url": "https://example.com/cat.png", "detail": "low"}},
    ]);
    let urls: Vec<&str> = image_urls(&content).collect();
    assert_eq!(
        urls,
        ["data:image/png;base64,AAAA", "https://example.com/cat.png"]
    );
}

#[test]
fn image_parts_are_read_from_among_text_parts() {
    let content = json!([
        {"type": "text", "text": "what is this?"},
        {"type": "image_url", "image_url": {"url": "u1"}},
        {"type": "text", "text": "and this?"},
        {"type": "image_url", "image_url": {"url": "u2"}},
    ]);
    assert_eq!(image_urls(&content).collect::<Vec<_>>(), ["u1", "u2"]);
    assert_eq!(text_parts(&content), 2, "the text walk is unchanged");
}

#[test]
fn an_image_url_that_is_a_bare_string_is_read() {
    let content = json!([
        {"type": "image_url", "image_url": "data:image/jpeg;base64,BBBB"},
        {"type": "image_url", "image_url": {"url": "u2"}},
    ]);
    let urls: Vec<&str> = image_urls(&content).collect();
    assert_eq!(urls, ["data:image/jpeg;base64,BBBB", "u2"]);
}

#[test]
fn content_with_no_image_part_carries_none() {
    for content in [
        json!(null),
        json!(42),
        json!({"image_url": {"url": "not in an array"}}),
        json!([]),
        json!([{"type": "text", "text": "image_url"}]),
        json!([{"type": "image_url"}]),
        json!([{"type": "image_url", "image_url": {"detail": "low"}}]),
        json!([{"type": "image_url", "image_url": {"url": 7}}]),
        json!([{"type": "image_url", "image_url": null}]),
    ] {
        assert_eq!(image_urls(&content).count(), 0, "{content}");
    }
}
