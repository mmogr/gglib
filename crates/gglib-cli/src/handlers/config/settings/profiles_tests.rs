//! Tests for [`super`] — the profile merge, the summary line and the
//! not-found message.

use super::*;

fn config() -> InferenceConfig {
    InferenceConfig {
        temperature: Some(0.2),
        top_k: Some(40),
        ..Default::default()
    }
}

/// A `set` invocation must only touch the parameters it names — that is
/// what makes editing one field of an existing profile safe.
#[test]
fn merge_set_only_touches_named_parameters() {
    let mut target = config();
    merge_set(
        &mut target,
        &InferenceConfig {
            temperature: Some(0.9),
            ..Default::default()
        },
    );

    assert_eq!(target.temperature, Some(0.9), "named parameter is updated");
    assert_eq!(target.top_k, Some(40), "unnamed parameter is preserved");
}

#[test]
fn summarize_lists_only_what_is_set() {
    let summary = summarize(&config());
    assert!(summary.contains("temperature=0.2"), "got: {summary}");
    assert!(summary.contains("top-k=40"), "got: {summary}");
    assert!(
        !summary.contains("min-p"),
        "unset params omitted: {summary}"
    );
    assert!(summarize(&InferenceConfig::default()).is_empty());
}

/// `gglib config profile list` is the only place a stored reasoning
/// control is visible at all — neither field is echoed by llama-server, so
/// a profile omitted from this line is a setting with no surface.
#[test]
fn summarize_names_both_reasoning_controls() {
    let summary = summarize(&InferenceConfig {
        reasoning_effort: Some(gglib_core::domain::ReasoningEffort::XHigh),
        reasoning_budget_tokens: Some(-1),
        ..Default::default()
    });

    assert!(summary.contains("reasoning-effort=xhigh"), "got: {summary}");
    assert!(
        summary.contains("reasoning-budget-tokens=-1"),
        "got: {summary}"
    );
}

/// A profile that set only an effort would merge into an all-`None`
/// config on any surface that forgot the field, which reads identically
/// to "no parameters set". This pins that `set` carries both.
#[test]
fn merge_set_carries_both_reasoning_controls() {
    let mut target = InferenceConfig::default();
    merge_set(
        &mut target,
        &InferenceConfig {
            reasoning_effort: Some(gglib_core::domain::ReasoningEffort::Low),
            reasoning_budget_tokens: Some(0),
            ..Default::default()
        },
    );

    assert_eq!(
        target.reasoning_effort,
        Some(gglib_core::domain::ReasoningEffort::Low)
    );
    assert_eq!(target.reasoning_budget_tokens, Some(0));
}

#[test]
fn not_found_message_lists_what_exists() {
    let profiles = vec![InferenceProfile {
        name: "coding".to_owned(),
        description: None,
        config: InferenceConfig::default(),
        list_in_models: false,
    }];
    let message = not_found_message("codeing", &profiles);
    assert!(message.contains("codeing"), "names the miss: {message}");
    assert!(
        message.contains("coding"),
        "names the alternative: {message}"
    );

    let empty = not_found_message("coding", &[]);
    assert!(empty.contains("install-templates"), "got: {empty}");
}
