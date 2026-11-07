//! What a settings patch refuses, what it changes, and what it reads.

use super::*;
use crate::domain::Machine;

fn local(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

/// Settings a conversation already holds: a model, a tool list and limits.
fn held() -> ConversationSettings {
    ConversationSettings {
        model_name: Some("qwen3".to_owned()),
        model: Some(local(1)),
        tools: vec!["files".to_owned()],
        max_parallel: Some(4),
        tool_timeout_ms: Some(5_000),
        temperature: Some(0.2),
        ..ConversationSettings::default()
    }
}

#[test]
fn limits_inside_a_runs_bounds_are_not_refused() {
    let patch = ConversationSettingsPatch {
        max_parallel: Some(Some(50)),
        tool_timeout_ms: Some(Some(100)),
        ..ConversationSettingsPatch::default()
    };
    assert_eq!(patch.refusal(), None);
    let patch = ConversationSettingsPatch {
        max_parallel: Some(Some(1)),
        tool_timeout_ms: Some(Some(60_000)),
        ..ConversationSettingsPatch::default()
    };
    assert_eq!(patch.refusal(), None);
}

#[test]
fn a_limit_outside_a_runs_bounds_is_refused_with_them() {
    let parallel = |n| ConversationSettingsPatch {
        max_parallel: Some(Some(n)),
        ..ConversationSettingsPatch::default()
    };
    let timeout = |ms| ConversationSettingsPatch {
        tool_timeout_ms: Some(Some(ms)),
        ..ConversationSettingsPatch::default()
    };
    assert_eq!(
        parallel(0).refusal().as_deref(),
        Some("max_parallel must be from 1 to 50, not 0")
    );
    assert_eq!(
        parallel(51).refusal().as_deref(),
        Some("max_parallel must be from 1 to 50, not 51")
    );
    assert_eq!(
        timeout(99).refusal().as_deref(),
        Some("tool_timeout_ms must be from 100 to 60000, not 99")
    );
    assert_eq!(
        timeout(60_001).refusal().as_deref(),
        Some("tool_timeout_ms must be from 100 to 60000, not 60001")
    );
}

#[test]
fn an_empty_patch_changes_nothing() {
    let patch = ConversationSettingsPatch::default();
    assert!(patch.is_empty());
    let mut settings = held();
    patch.apply(&mut settings, Some("other".to_owned()));
    assert_eq!(settings, held());
}

#[test]
fn a_new_model_brings_its_name_and_keeps_the_rest() {
    let mut settings = held();
    let patch = ConversationSettingsPatch {
        model: Some(Some(local(7))),
        ..ConversationSettingsPatch::default()
    };
    assert!(!patch.is_empty());
    patch.apply(&mut settings, None);
    let expected = ConversationSettings {
        model: Some(local(7)),
        model_name: None,
        ..held()
    };
    assert_eq!(settings, expected);
}

#[test]
fn each_field_changes_only_itself() {
    let mut settings = held();
    ConversationSettingsPatch {
        tools: Some(Vec::new()),
        ..ConversationSettingsPatch::default()
    }
    .apply(&mut settings, None);
    assert_eq!(
        settings,
        ConversationSettings {
            tools: Vec::new(),
            ..held()
        }
    );

    let mut settings = held();
    ConversationSettingsPatch {
        max_parallel: Some(Some(9)),
        ..ConversationSettingsPatch::default()
    }
    .apply(&mut settings, None);
    assert_eq!(
        settings,
        ConversationSettings {
            max_parallel: Some(9),
            ..held()
        }
    );

    let mut settings = held();
    ConversationSettingsPatch {
        tool_timeout_ms: Some(Some(700)),
        ..ConversationSettingsPatch::default()
    }
    .apply(&mut settings, None);
    assert_eq!(
        settings,
        ConversationSettings {
            tool_timeout_ms: Some(700),
            ..held()
        }
    );
}

/// Off is stored as `true`; on again removes the key, as in a conversation
/// that never switched tools off.
#[test]
fn tools_switched_off_and_on_again() {
    let mut settings = held();
    let switch = |on: bool| ConversationSettingsPatch {
        no_tools: Some(!on),
        ..ConversationSettingsPatch::default()
    };
    switch(false).apply(&mut settings, None);
    assert_eq!(settings.no_tools, Some(true));
    switch(true).apply(&mut settings, None);
    assert_eq!(settings, held());
}

#[test]
fn a_key_it_does_not_name_is_refused() {
    let read: Result<ConversationSettingsPatch, _> =
        serde_json::from_str(r#"{"temperature": 0.5}"#);
    assert!(read.is_err(), "{read:?}");
    let read: ConversationSettingsPatch =
        serde_json::from_str(r#"{"max_parallel": 3, "no_tools": true}"#).unwrap();
    assert_eq!(
        read,
        ConversationSettingsPatch {
            max_parallel: Some(Some(3)),
            no_tools: Some(true),
            ..ConversationSettingsPatch::default()
        }
    );
}

/// A `null` is read as a clear, and a key left out as no change.
#[test]
fn a_null_clears_and_a_key_left_out_keeps() {
    let read: ConversationSettingsPatch =
        serde_json::from_str(r#"{"model": null, "max_parallel": null, "tool_timeout_ms": null}"#)
            .unwrap();
    assert_eq!(
        read,
        ConversationSettingsPatch {
            model: Some(None),
            max_parallel: Some(None),
            tool_timeout_ms: Some(None),
            ..ConversationSettingsPatch::default()
        }
    );
    let read: ConversationSettingsPatch = serde_json::from_str("{}").unwrap();
    assert!(read.is_empty());
}

#[test]
fn a_cleared_model_takes_its_name_and_cleared_limits_their_defaults() {
    let mut settings = held();
    let patch = ConversationSettingsPatch {
        model: Some(None),
        max_parallel: Some(None),
        tool_timeout_ms: Some(None),
        ..ConversationSettingsPatch::default()
    };
    assert_eq!(patch.refusal(), None);
    patch.apply(&mut settings, None);
    let expected = ConversationSettings {
        model: None,
        model_name: None,
        max_parallel: None,
        tool_timeout_ms: None,
        ..held()
    };
    assert_eq!(settings, expected);
}
