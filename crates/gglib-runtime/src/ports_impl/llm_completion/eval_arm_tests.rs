//! The two things an A/B evaluation asks of the adapter's request path: a
//! control arm that skips the pipeline, and a `tool_choice` demanded of the
//! opening turn alone. Split from `shaping_tests.rs`, unchanged, when that
//! file was at its budget.

use super::*;

/// The eval's control arm: no pipeline at all. No sampling keys resolve in
/// (the upstream's own defaults apply) and `cache_prompt` is never pinned.
#[test]
fn raw_passthrough_skips_the_pipeline_entirely() {
    let adapter = LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
        .with_raw_passthrough(true);
    let body = body_of(&adapter, &[user("hi")]);

    assert!(body.get("temperature").is_none(), "no sampling floor");
    assert!(body.get("cache_prompt").is_none(), "no pinning");
    assert_eq!(body["stream"], true, "transport fields still present");
}

/// A caller-set `tool_choice` lands in the body where the pipeline (and the
/// upstream) read it, overriding `build_chat_body`'s `auto` default — but only
/// on the opening turn.
///
/// Holding a model at `required` for every turn makes a final answer
/// unreachable: it must emit a tool call, so it re-emits its last batch until
/// the loop guard aborts the run. The eval that motivated this tripped its
/// guard on all seven tool-demanding tasks for exactly that reason.
#[test]
fn tool_choice_applies_to_the_first_turn_only() {
    let tools = vec![gglib_core::domain::agent::ToolDefinition {
        name: "f".to_owned(),
        description: None,
        input_schema: Some(json!({"type": "object"})),
        title: None,
        deadline: None,
    }];
    let adapter = LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
        .with_raw_passthrough(true)
        .with_first_turn_tool_choice(Some("required".to_owned()));

    let first = adapter
        .shaped_body(&[user("hi")], &tools, &ImageUrls::default())
        .unwrap();
    assert_eq!(first["tool_choice"], "required", "opening turn carries it");

    let second = adapter
        .shaped_body(&[user("hi")], &tools, &ImageUrls::default())
        .unwrap();
    assert_eq!(
        second["tool_choice"], "auto",
        "later turns fall back to the default, so the model can finish"
    );
}

/// With no tools there is no `auto` default to fall back to, so the key is
/// absent entirely rather than left pinned from the opening turn.
#[test]
fn a_spent_tool_choice_leaves_no_key_behind() {
    let adapter = LlmCompletionAdapter::new("http://127.0.0.1:0", Some("m".to_owned()))
        .with_raw_passthrough(true)
        .with_first_turn_tool_choice(Some("required".to_owned()));

    let first = adapter
        .shaped_body(&[user("hi")], &[], &ImageUrls::default())
        .unwrap();
    assert_eq!(first["tool_choice"], "required");

    let second = adapter
        .shaped_body(&[user("hi")], &[], &ImageUrls::default())
        .unwrap();
    assert!(second.get("tool_choice").is_none());
}
