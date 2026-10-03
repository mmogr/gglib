//! Tests for [`super::derive_fallback_session_id`].

use bytes::Bytes;
use serde_json::{Value, json};

use super::derive_fallback_session_id;

fn body_with(system: &str, user: &str) -> Bytes {
    Bytes::from(
        serde_json::to_vec(&json!({
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        }))
        .unwrap(),
    )
}

#[test]
fn fallback_session_id_stable_across_turns() {
    let turn1 = body_with("You are the Planner.", "Design a login flow");
    let turn2 = Bytes::from(
        serde_json::to_vec(&json!({
            "messages": [
                {"role": "system", "content": "You are the Planner."},
                {"role": "user", "content": "Design a login flow"},
                {"role": "assistant", "content": "Here's a plan..."},
                {"role": "user", "content": "Now refine step 2"}
            ]
        }))
        .unwrap(),
    );
    let id1 = derive_fallback_session_id(&turn1).unwrap();
    let id2 = derive_fallback_session_id(&turn2).unwrap();
    assert_eq!(
        id1, id2,
        "same agent/task should map to the same bucket across turns"
    );
}

#[test]
fn fallback_session_id_differs_by_role() {
    let planner = body_with("You are the Planner.", "Design a login flow");
    let coder = body_with("You are the Coder.", "Design a login flow");
    assert_ne!(
        derive_fallback_session_id(&planner).unwrap(),
        derive_fallback_session_id(&coder).unwrap()
    );
}

#[test]
fn fallback_session_id_differs_by_task() {
    let task_a = body_with("You are the Coder.", "Implement login");
    let task_b = body_with("You are the Coder.", "Implement logout");
    assert_ne!(
        derive_fallback_session_id(&task_a).unwrap(),
        derive_fallback_session_id(&task_b).unwrap()
    );
}

#[test]
fn fallback_session_id_ignores_dynamic_lines() {
    // derive_fallback_session_id requires pre-canonicalized input (see its
    // doc comment) — the caller (chat_completions) canonicalizes once up
    // front. Mirror that contract here rather than passing raw bodies.
    let with_timestamp = crate::canonicalization::canonicalize_system_prompt(body_with(
        "You are an assistant.\nCurrent date: 2026-07-15\nMore instructions.",
        "Hello",
    ));
    let without_timestamp = crate::canonicalization::canonicalize_system_prompt(body_with(
        "You are an assistant.\nMore instructions.",
        "Hello",
    ));
    assert_eq!(
        derive_fallback_session_id(&with_timestamp).unwrap(),
        derive_fallback_session_id(&without_timestamp).unwrap(),
        "dynamic IDE-injected lines must not change the fingerprint turn to turn"
    );
}

#[test]
fn fallback_session_id_handles_array_form_content() {
    let string_form = body_with("You are the Coder.", "Implement login");
    let array_form = Bytes::from(
        serde_json::to_vec(&json!({
            "messages": [
                {"role": "system", "content": [{"type": "text", "text": "You are the Coder."}]},
                {"role": "user", "content": [{"type": "text", "text": "Implement login"}]}
            ]
        }))
        .unwrap(),
    );
    assert_eq!(
        derive_fallback_session_id(&string_form).unwrap(),
        derive_fallback_session_id(&array_form).unwrap(),
        "string and array content forms carrying the same text must fingerprint identically"
    );
}

#[test]
fn fallback_session_id_none_without_messages() {
    let body = Bytes::from(serde_json::to_vec(&json!({"foo": "bar"})).unwrap());
    assert!(derive_fallback_session_id(&body).is_none());
}

#[test]
fn fallback_session_id_none_on_invalid_json() {
    let body = Bytes::from(b"not json".to_vec());
    assert!(derive_fallback_session_id(&body).is_none());
}

#[test]
fn fallback_session_id_is_valid_for_sanitize() {
    let body = body_with("You are the Coder.", "Implement login");
    let id = derive_fallback_session_id(&body).unwrap();
    crate::slots::sanitize_session_id(&id).expect("derived id must pass sanitize_session_id");
}

/// One user message: `text`, then an image part for each of `urls`.
fn body_with_images(text: &str, urls: &[&str]) -> Bytes {
    let mut parts = vec![json!({"type": "text", "text": text})];
    parts.extend(
        urls.iter()
            .map(|url| json!({"type": "image_url", "image_url": {"url": url}})),
    );
    let body: Value = json!({"messages": [{"role": "user", "content": parts}]});
    Bytes::from(serde_json::to_vec(&body).unwrap())
}

/// The ids this function gave before it read images, computed by running it
/// at the commit before: a chat with no image keeps its id, and with it its
/// saved KV slot and its frozen budget.
#[test]
fn a_text_only_chat_keeps_the_id_it_had_before_images_were_hashed() {
    let string_form = body_with("You are a coding agent.", "Why does the build fail?");
    assert_eq!(
        derive_fallback_session_id(&string_form).as_deref(),
        Some("auto-051c6449182fbe5c8ab70ee17d7d3c9b")
    );
    let array_form = body_with_images("Why does the build fail?", &[]);
    assert_eq!(
        derive_fallback_session_id(&array_form).as_deref(),
        Some("auto-2fe987e1d2b1fb348bb200b6f64a5d7e")
    );
}

#[test]
fn the_same_words_about_different_images_are_different_chats() {
    let id = |urls: &[&str]| derive_fallback_session_id(&body_with_images("What is this?", urls));
    let first = id(&["data:image/png;base64,AAAA"]);
    assert!(first.is_some());
    assert_ne!(first, id(&["data:image/png;base64,BBBB"]));
    assert_ne!(first, id(&[]), "an image is not no image");
    assert_ne!(
        first,
        id(&["data:image/png;base64,AAAA", "data:image/png;base64,AAAA"]),
        "one image is not two"
    );
    assert_ne!(
        id(&["data:image/png;base64,AA", "AA"]),
        id(&["data:image/png;base64,AAA", "A"]),
        "where one URL ends is part of the id"
    );
}

#[test]
fn the_same_words_about_the_same_image_are_the_same_chat() {
    let first_turn = body_with_images("What is this?", &["data:image/png;base64,AAAA"]);
    let later_turn = Bytes::from(
        serde_json::to_vec(&json!({"messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "What is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
            ]},
            {"role": "assistant", "content": "A cat."},
            {"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,CCCC"}},
            ]},
        ]}))
        .unwrap(),
    );
    let id = derive_fallback_session_id(&first_turn);
    assert!(id.is_some());
    assert_eq!(id, derive_fallback_session_id(&later_turn));
    // A bare-string `image_url` is the same image.
    let bare = Bytes::from(
        serde_json::to_vec(&json!({"messages": [{"role": "user", "content": [
            {"type": "text", "text": "What is this?"},
            {"type": "image_url", "image_url": "data:image/png;base64,AAAA"},
        ]}]}))
        .unwrap(),
    );
    assert_eq!(id, derive_fallback_session_id(&bare));
}

#[test]
fn a_chat_that_opens_with_an_image_and_no_words_has_an_id() {
    let id = |urls: &[&str]| derive_fallback_session_id(&body_with_images("", urls));
    assert_eq!(id(&[]), None);
    let one = id(&["data:image/png;base64,AAAA"]);
    assert!(one.is_some());
    assert_ne!(one, id(&["data:image/png;base64,BBBB"]));
}
