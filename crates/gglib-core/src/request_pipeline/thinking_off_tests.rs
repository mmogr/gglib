//! Tests for [`super`]: which budget reads as thinking off. What the
//! pipeline then sends is `apply_thinking_off_tests.rs`.

use serde_json::json;

use super::*;

#[test]
fn only_an_integer_budget_of_zero_turns_thinking_off() {
    for off in [
        json!({"reasoning_budget_tokens": 0}),
        json!({"thinking_budget_tokens": 0}),
        json!({"reasoning_budget_tokens": null, "thinking_budget_tokens": 0}),
        json!({"reasoning_budget_tokens": "0", "thinking_budget_tokens": 0}),
    ] {
        assert!(budget_turns_thinking_off(&off), "{off}");
    }
    for on in [
        json!({}),
        json!({"reasoning_budget_tokens": 512}),
        json!({"reasoning_budget_tokens": -1}),
        json!({"reasoning_budget_tokens": 512, "thinking_budget_tokens": 0}),
        json!({"reasoning_budget_tokens": "0"}),
        json!({"reasoning_budget_tokens": 0.5}),
        json!({"reasoning_budget_tokens": false}),
        json!("not an object"),
    ] {
        assert!(!budget_turns_thinking_off(&on), "{on}");
    }
}

#[test]
fn the_flag_is_set_only_where_the_budget_is_zero() {
    let mut off = json!({"reasoning_budget_tokens": 0, "chat_template_kwargs": {"x": 1}});
    assert!(tell_template_thinking_is_off(&mut off));
    assert_eq!(
        off["chat_template_kwargs"],
        json!({"x": 1, "enable_thinking": false})
    );

    let mut on = json!({"reasoning_budget_tokens": 512});
    assert!(!tell_template_thinking_is_off(&mut on));
    assert_eq!(on, json!({"reasoning_budget_tokens": 512}));

    let mut not_an_object = json!({"reasoning_budget_tokens": 0, "chat_template_kwargs": [1]});
    assert!(!tell_template_thinking_is_off(&mut not_an_object));
    assert_eq!(not_an_object["chat_template_kwargs"], json!([1]));
}
