//! Tests for [`super`] — metadata parsing and the update merge.

use super::*;
use std::path::PathBuf;

fn create_test_model() -> Model {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), "Test Model".to_string());
    metadata.insert("test.key".to_string(), "test.value".to_string());

    Model {
        dialect_spec: None,
        id: 1,
        name: "Original Name".to_string(),
        model_key: String::new(),
        file_path: PathBuf::from("/test/model.gguf"),
        param_count_b: 7.0,
        inference_defaults: None,
        defaults_origin: None,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        expert_count: None,
        expert_used_count: None,
        expert_shared_count: None,
        metadata,
        added_at: chrono::Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        capabilities: gglib_core::ModelCapabilities::default(),
        last_update_check: None,
        tags: Vec::new(),
        server_defaults: None,
        template_caps: None,
        benchmark_summary: None,
    }
}

#[test]
fn test_parse_metadata_updates() {
    let metadata_args = vec![
        "key1=value1".to_string(),
        "key2=value2".to_string(),
        "complex.key=complex value with spaces".to_string(),
    ];

    let result = parse_metadata_updates(&metadata_args).unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result.get("key1"), Some(&"value1".to_string()));
    assert_eq!(result.get("key2"), Some(&"value2".to_string()));
    assert_eq!(
        result.get("complex.key"),
        Some(&"complex value with spaces".to_string())
    );
}

#[test]
fn test_parse_metadata_updates_invalid_format() {
    let metadata_args = vec!["invalid_format".to_string()];
    let result = parse_metadata_updates(&metadata_args);
    assert!(result.is_err());
}

#[test]
fn test_parse_metadata_removals() {
    let remove_arg = Some("key1,key2, key3 ".to_string());
    let result = parse_metadata_removals(&remove_arg).unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result, vec!["key1", "key2", "key3"]);
}

/// An `UpdateArgs` that asks for nothing, as the base for a focused test.
///
/// Written as a full literal rather than a `Default` impl so that adding a
/// field to `UpdateArgs` breaks here, and each test below says only what it
/// actually wants.
fn bare_args() -> UpdateArgs {
    UpdateArgs {
        identifier: "1".to_string(),
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: Vec::new(),
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: false,
        temperature: None,
        top_p: None,
        top_k: None,
        max_tokens: None,
        repeat_penalty: None,
        presence_penalty: None,
        min_p: None,
        dry_multiplier: None,
        dry_base: None,
        dry_allowed_length: None,
        dry_penalty_last_n: None,
        dynatemp_range: None,
        dynatemp_exponent: None,
        top_n_sigma: None,
        frequency_penalty: None,
        reasoning_effort: None,
        reasoning_budget_tokens: None,
        unset: Vec::new(),
        clear_inference_defaults: false,
    }
}

/// A model whose stored defaults an `--unset` has something to bite on.
fn model_with_defaults(config: InferenceConfig) -> Model {
    Model {
        inference_defaults: Some(config),
        defaults_origin: Some(DefaultsOrigin::User),
        ..create_test_model()
    }
}

fn apply(existing: &Model, args: &UpdateArgs) -> Model {
    let updates = parse_metadata_updates(&args.metadata).expect("metadata parses");
    let removals = parse_metadata_removals(&args.remove_metadata).expect("removals parse");
    create_updated_model(existing, args, &updates, &removals).expect("update applies")
}

#[test]
fn test_create_updated_model() {
    let updated = apply(
        &create_test_model(),
        &UpdateArgs {
            name: Some("Updated Name".to_string()),
            param_count: Some(13.0),
            architecture: Some("mistral".to_string()),
            context_length: Some(8192),
            metadata: vec!["new.key=new.value".to_string()],
            remove_metadata: Some("test.key".to_string()),
            ..bare_args()
        },
    );

    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.param_count_b, 13.0);
    assert_eq!(updated.architecture, Some("mistral".to_string()));
    assert_eq!(updated.quantization, Some("Q4_0".to_string())); // Unchanged
    assert_eq!(updated.context_length, Some(8192));
    assert!(updated.metadata.contains_key("new.key"));
    assert!(!updated.metadata.contains_key("test.key")); // Removed
}

#[test]
fn both_reasoning_controls_are_stored() {
    let updated = apply(
        &create_test_model(),
        &UpdateArgs {
            reasoning_effort: Some(ReasoningEffort::XHigh),
            reasoning_budget_tokens: Some(-1),
            ..bare_args()
        },
    );

    let stored = updated.inference_defaults.expect("defaults written");
    assert_eq!(stored.reasoning_effort, Some(ReasoningEffort::XHigh));
    assert_eq!(
        stored.reasoning_budget_tokens,
        Some(-1),
        "-1 is a value, not an absence"
    );
    assert_eq!(
        updated.defaults_origin,
        Some(DefaultsOrigin::User),
        "a deliberate flag makes the row user-set"
    );
}

/// The point of `--unset`: dial one parameter back to falling through without
/// disturbing the others. `--clear-inference-defaults` is the blunt version,
/// and until now it was the only version.
#[test]
fn unset_clears_one_parameter_and_leaves_the_rest() {
    let existing = model_with_defaults(InferenceConfig {
        temperature: Some(0.2),
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(16384),
        ..Default::default()
    });

    let updated = apply(
        &existing,
        &UpdateArgs {
            unset: vec!["reasoning-effort".to_string()],
            ..bare_args()
        },
    );

    let stored = updated.inference_defaults.expect("row survives");
    assert_eq!(stored.reasoning_effort, None, "cleared");
    assert_eq!(stored.reasoning_budget_tokens, Some(16384), "untouched");
    assert_eq!(stored.temperature, Some(0.2), "untouched");
}

/// Clears run after sets, so the last thing said about a parameter holds.
#[test]
fn unset_wins_over_a_value_set_in_the_same_invocation() {
    let updated = apply(
        &create_test_model(),
        &UpdateArgs {
            temperature: Some(0.9),
            top_k: Some(40),
            unset: vec!["temperature".to_string()],
            ..bare_args()
        },
    );

    let stored = updated.inference_defaults.expect("defaults written");
    assert_eq!(stored.temperature, None);
    assert_eq!(stored.top_k, Some(40));
}

/// Clearing the last parameter has to land back at *inherit*. An all-`None`
/// row is not the same thing: it outranks global settings while saying
/// nothing, so `--unset` one at a time would otherwise reach a state
/// `--clear-inference-defaults` cannot produce.
#[test]
fn unsetting_the_last_parameter_returns_to_inherit() {
    let existing = model_with_defaults(InferenceConfig {
        temperature: Some(0.2),
        ..Default::default()
    });

    let updated = apply(
        &existing,
        &UpdateArgs {
            unset: vec!["temperature".to_string()],
            ..bare_args()
        },
    );

    assert_eq!(updated.inference_defaults, None);
    assert_eq!(
        updated.defaults_origin, None,
        "no value left to have an origin"
    );
}

/// An unknown name fails the whole update rather than being ignored — a
/// typo'd `--unset temprature` that silently did nothing would read as a
/// successful clear.
#[test]
fn an_unknown_unset_name_fails_the_update() {
    let args = UpdateArgs {
        unset: vec!["temprature".to_string()],
        ..bare_args()
    };
    let error = create_updated_model(&create_test_model(), &args, &HashMap::new(), &[])
        .expect_err("unknown parameter");

    assert!(error.to_string().contains("temprature"), "got: {error}");
}
