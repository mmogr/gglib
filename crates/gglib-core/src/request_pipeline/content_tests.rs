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
    assert_eq!(for_each_text_mut(&mut content, &mut upper), 1);
    assert_eq!(content[0], json!({"type": "text", "text": "LOOK"}));
    assert_eq!(content[1], image, "the shape and the other part are kept");
}

#[test]
fn an_array_with_no_text_part_carries_no_text() {
    let mut content = json!([{"type": "image_url", "image_url": {"url": "x"}}]);
    let before = content.clone();
    assert_eq!(text_len(&content), 0);
    assert_eq!(for_each_text_mut(&mut content, &mut upper), 0);
    assert_eq!(content, before);
}

#[test]
fn a_shape_that_is_neither_is_left_alone() {
    for mut content in [json!(null), json!(42), json!({"text": "not a part"})] {
        let before = content.clone();
        assert_eq!(text_len(&content), 0);
        assert_eq!(for_each_text_mut(&mut content, &mut upper), 0);
        assert_eq!(content, before);
    }
}
