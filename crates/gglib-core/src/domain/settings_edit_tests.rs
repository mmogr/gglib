use serde_json::json;

use super::*;
use crate::domain::Thinking;

#[test]
fn a_key_that_cannot_be_read_is_passed_over_and_the_rest_are_read() {
    let text = r#"{"model_name":"Qwen","max_tokens":"many","thinking":"off","later":[1]}"#;
    let read = ConversationSettings::read(text).expect("an object");
    assert_eq!(read.model_name.as_deref(), Some("Qwen"));
    assert_eq!(read.thinking, Some(Thinking::Off));
    assert_eq!(read.max_tokens, None);
}

#[test]
fn text_that_is_not_an_object_reads_as_none() {
    for text in ["not json", "[1]", "null", "\"off\""] {
        assert_eq!(ConversationSettings::read(text), None, "{text}");
    }
}

#[test]
fn an_edit_names_only_the_keys_that_change() {
    let held = ConversationSettings {
        model_name: Some("Qwen".into()),
        thinking: Some(Thinking::Off),
        max_parallel: Some(4),
        ..ConversationSettings::default()
    };
    let next = ConversationSettings {
        thinking: None,
        folder: Some("/work".into()),
        ..held.clone()
    };
    let edit = next.edit_from(&held);
    let expected: SettingsEdit = [
        ("folder".to_owned(), Some(json!("/work"))),
        ("thinking".to_owned(), None),
    ]
    .into();
    assert_eq!(edit, expected);
    assert!(held.edit_from(&held).is_empty());
}

#[test]
fn an_edit_with_the_model_names_the_model_and_its_name_unchanged() {
    let held = ConversationSettings {
        model_name: Some("Qwen".into()),
        max_tokens: Some(40),
        ..ConversationSettings::default()
    };
    let edit = held.edit_with_model_from(&held);
    let expected: SettingsEdit = [
        ("model".to_owned(), None),
        ("model_name".to_owned(), Some(json!("Qwen"))),
    ]
    .into();
    assert_eq!(edit, expected);
}
