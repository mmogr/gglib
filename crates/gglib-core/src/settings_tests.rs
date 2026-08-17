//! Tests for [`super::Settings`].
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;

#[test]
fn test_default_settings() {
    let settings = Settings::with_defaults();
    assert_eq!(settings.default_context_size, Some(4096));
    assert_eq!(settings.proxy_port, Some(DEFAULT_PROXY_PORT));
    assert_eq!(settings.llama_base_port, Some(DEFAULT_LLAMA_BASE_PORT));
    assert_eq!(settings.default_download_path, None);
    assert_eq!(settings.max_download_queue_size, Some(10));
    assert_eq!(settings.show_memory_fit_indicators, Some(true));
}

#[test]
fn test_validate_settings_valid() {
    let settings = Settings::with_defaults();
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn test_validate_context_size_too_small() {
    let settings = Settings {
        default_context_size: Some(100),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::InvalidContextSize(100))
    ));
}

#[test]
fn test_validate_context_size_too_large() {
    let settings = Settings {
        default_context_size: Some(2_000_000),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::InvalidContextSize(2_000_000))
    ));
}

#[test]
fn test_validate_port_too_low() {
    let settings = Settings {
        proxy_port: Some(80),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::InvalidPort(80))
    ));
}

#[test]
fn test_validate_empty_path() {
    let settings = Settings {
        default_download_path: Some(String::new()),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::EmptyDownloadPath)
    ));
}

#[test]
fn test_validate_inference_config_valid() {
    let config = InferenceConfig {
        temperature: Some(0.7),
        top_p: Some(0.9),
        top_k: Some(40),
        max_tokens: Some(2048),
        repeat_penalty: Some(1.1),
        presence_penalty: Some(0.0),
        min_p: Some(0.0),
        dry_multiplier: Some(0.8),
        dry_base: Some(1.75),
        dry_allowed_length: Some(2),
        dry_penalty_last_n: Some(-1),
        dynatemp_range: Some(0.5),
        dynatemp_exponent: Some(1.0),
        top_n_sigma: Some(1.0),
        frequency_penalty: Some(0.4),
        seed: None,
        // `reasoning_effort` is an enum, so serde refuses an unknown level
        // before this function sees the config; there is nothing left to range
        // check. `reasoning_budget_tokens` is a bounded integer and is checked
        // here like every other one — see the tests below.
        reasoning_effort: None,
        reasoning_budget_tokens: Some(0),
    };
    assert!(validate_inference_config(&config).is_ok());
}

/// The sentinels are both valid, and neither is "no opinion".
///
/// `-1` defers to the launch `--reasoning-budget` and `0` stops thinking
/// immediately — upstream accepts both, so a guard that rejected either would
/// be gglib inventing a narrower range than the system it forwards to.
#[test]
fn test_validate_inference_config_reasoning_budget_sentinels() {
    for budget in [-1, 0, 1, i32::MAX] {
        let config = InferenceConfig {
            reasoning_budget_tokens: Some(budget),
            ..Default::default()
        };
        assert!(
            validate_inference_config(&config).is_ok(),
            "{budget} is inside upstream's range and must store"
        );
    }
}

/// Stored configuration is range-checked, not just request parameters.
///
/// Below `-1` llama-server answers HTTP 400 naming the range (ADR 0007
/// finding 7c). `extract_client_sampling` reproduces that verdict for a value
/// that arrives on a request, but `Settings::inference_defaults`, an inference
/// profile's `config` and the proxy's `inference_override` all deserialise a
/// whole `InferenceConfig` straight from JSON and never reach it. Without this
/// guard a stored `-5000` is force-inserted into every chat body and 400s every
/// request to every model, with nothing failing at store time and no readback
/// that can point at the field — both reasoning controls are permanently Blind
/// (ADR 0007 finding 7a).
#[test]
fn test_validate_inference_config_reasoning_budget_below_upstream_range() {
    for budget in [-2, -5000, i32::MIN] {
        let config = InferenceConfig {
            reasoning_budget_tokens: Some(budget),
            ..Default::default()
        };
        let err = validate_inference_config(&config)
            .expect_err("a value upstream answers 400 for must not store");
        assert!(
            err.contains("Reasoning budget") && err.contains(&budget.to_string()),
            "the error must name the field and the value it refused: {err}"
        );
    }
}

/// The guard is reachable from the settings surface, not just callable.
///
/// `validate_settings` is what `gglib config settings set` and the settings
/// service call; the global rung and every profile rung run through it. A guard
/// that only the unit test above reaches would leave both ingress paths open.
#[test]
fn test_validate_settings_rejects_a_stored_reasoning_budget_below_range() {
    let bad = InferenceConfig {
        reasoning_budget_tokens: Some(-2),
        ..Default::default()
    };

    let settings = Settings {
        inference_defaults: Some(bad.clone()),
        ..Settings::with_defaults()
    };
    assert!(
        validate_settings(&settings).is_err(),
        "the global rung must not accept a budget upstream rejects"
    );

    let settings = Settings {
        inference_profiles: Some(vec![InferenceProfile {
            name: "coding".to_string(),
            description: None,
            config: bad,
            list_in_models: false,
        }]),
        ..Settings::with_defaults()
    };
    assert!(
        validate_settings(&settings).is_err(),
        "a profile rung must not accept a budget upstream rejects"
    );
}

#[test]
fn test_validate_inference_config_temperature_out_of_range() {
    let config = InferenceConfig {
        temperature: Some(2.5),
        ..Default::default()
    };
    assert!(validate_inference_config(&config).is_err());

    let config = InferenceConfig {
        temperature: Some(-0.1),
        ..Default::default()
    };
    assert!(validate_inference_config(&config).is_err());
}

#[test]
fn test_validate_inference_config_top_p_out_of_range() {
    let config = InferenceConfig {
        top_p: Some(1.5),
        ..Default::default()
    };
    assert!(validate_inference_config(&config).is_err());

    let config = InferenceConfig {
        top_p: Some(-0.1),
        ..Default::default()
    };
    assert!(validate_inference_config(&config).is_err());
}

#[test]
fn test_validate_inference_config_negative_values() {
    let config = InferenceConfig {
        top_k: Some(-1),
        ..Default::default()
    };
    assert!(validate_inference_config(&config).is_err());

    let config = InferenceConfig {
        repeat_penalty: Some(0.0),
        ..Default::default()
    };
    assert!(validate_inference_config(&config).is_err());
}

#[test]
fn test_settings_with_valid_inference_defaults() {
    let settings = Settings {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.8),
            top_p: Some(0.95),
            ..Default::default()
        }),
        ..Settings::with_defaults()
    };
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn test_settings_with_invalid_inference_defaults() {
    let settings = Settings {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(3.0), // Invalid
            ..Default::default()
        }),
        ..Settings::with_defaults()
    };
    assert!(validate_settings(&settings).is_err());
}

#[test]
fn test_validate_queue_size_too_small() {
    let settings = Settings {
        max_download_queue_size: Some(0),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::InvalidQueueSize(0))
    ));
}

#[test]
fn test_validate_queue_size_too_large() {
    let settings = Settings {
        max_download_queue_size: Some(100),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::InvalidQueueSize(100))
    ));
}

#[test]
fn test_merge_settings() {
    let mut settings = Settings::with_defaults();
    let update = SettingsUpdate {
        default_context_size: Some(Some(8192)),
        proxy_port: Some(None), // Clear proxy port
        ..Default::default()
    };
    settings.merge(&update);

    assert_eq!(settings.default_context_size, Some(8192));
    assert_eq!(settings.proxy_port, None);
    assert_eq!(settings.llama_base_port, Some(DEFAULT_LLAMA_BASE_PORT)); // Unchanged
}

#[test]
fn test_trust_client_sampling_defaults_to_none_and_merges_like_any_bool_setting() {
    let defaults = Settings::with_defaults();
    assert_eq!(defaults.trust_client_sampling, None);

    let mut settings = Settings::with_defaults();
    settings.merge(&SettingsUpdate {
        trust_client_sampling: Some(Some(true)),
        ..Default::default()
    });
    assert_eq!(settings.trust_client_sampling, Some(true));

    settings.merge(&SettingsUpdate {
        trust_client_sampling: Some(None),
        ..Default::default()
    });
    assert_eq!(settings.trust_client_sampling, None);
}

#[test]
fn test_proxy_loop_detection_defaults_to_none_and_merges_like_any_bool_setting() {
    // None means enabled — the guard is on unless explicitly disabled.
    let defaults = Settings::with_defaults();
    assert_eq!(defaults.proxy_loop_detection, None);

    let mut settings = Settings::with_defaults();
    settings.merge(&SettingsUpdate {
        proxy_loop_detection: Some(Some(false)),
        ..Default::default()
    });
    assert_eq!(settings.proxy_loop_detection, Some(false));

    settings.merge(&SettingsUpdate {
        proxy_loop_detection: Some(None),
        ..Default::default()
    });
    assert_eq!(settings.proxy_loop_detection, None);
}

#[test]
fn test_effective_ports() {
    let settings = Settings::with_defaults();
    assert_eq!(settings.effective_proxy_port(), DEFAULT_PROXY_PORT);
    assert_eq!(
        settings.effective_llama_base_port(),
        DEFAULT_LLAMA_BASE_PORT
    );

    let settings_none = Settings::default();
    assert_eq!(settings_none.effective_proxy_port(), DEFAULT_PROXY_PORT);
    assert_eq!(
        settings_none.effective_llama_base_port(),
        DEFAULT_LLAMA_BASE_PORT
    );
}

// ── Inference profiles ──────────────────────────────────────────────

fn profile(name: &str, temperature: f32) -> InferenceProfile {
    InferenceProfile {
        name: name.to_owned(),
        description: None,
        config: InferenceConfig {
            temperature: Some(temperature),
            ..Default::default()
        },
        list_in_models: false,
    }
}

#[test]
fn test_builtin_templates_pass_settings_validation() {
    let settings = Settings {
        inference_profiles: Some(crate::domain::builtin_templates()),
        ..Settings::with_defaults()
    };
    assert!(validate_settings(&settings).is_ok());
}

#[test]
fn test_validate_profiles_rejects_duplicate_names() {
    let err = validate_inference_profiles(&[profile("coding", 0.2), profile("coding", 0.9)])
        .expect_err("duplicates must be rejected");
    assert!(err.contains("duplicate"), "unexpected message: {err}");
    assert!(err.contains("coding"), "message should name the profile");
}

#[test]
fn test_validate_profiles_rejects_invalid_name() {
    let err = validate_inference_profiles(&[profile("Not_A_Slug", 0.5)])
        .expect_err("invalid slug must be rejected");
    assert!(err.contains("Not_A_Slug"), "unexpected message: {err}");
}

/// Profile parameters go through the same range checks as global
/// defaults, and the failure names which profile was at fault.
#[test]
fn test_validate_profiles_reuses_inference_config_ranges() {
    let err = validate_inference_profiles(&[profile("coding", 5.0)])
        .expect_err("out-of-range temperature must be rejected");
    assert!(err.contains("coding"), "message should name the profile");
    assert!(err.contains("Temperature"), "unexpected message: {err}");
}

#[test]
fn test_settings_with_invalid_profile_fails_validation() {
    let settings = Settings {
        inference_profiles: Some(vec![profile("coding", 5.0)]),
        ..Settings::with_defaults()
    };
    assert!(matches!(
        validate_settings(&settings),
        Err(SettingsError::InvalidInferenceProfile(_))
    ));
}

#[test]
fn test_merge_replaces_and_clears_profiles() {
    let mut settings = Settings {
        inference_profiles: Some(vec![profile("coding", 0.2)]),
        ..Settings::with_defaults()
    };

    settings.merge(&SettingsUpdate {
        inference_profiles: Some(Some(vec![profile("chat", 0.7)])),
        ..Default::default()
    });
    let profiles = settings.inference_profiles.as_ref().expect("still set");
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].name, "chat");

    settings.merge(&SettingsUpdate {
        inference_profiles: Some(None),
        ..Default::default()
    });
    assert_eq!(settings.inference_profiles, None);

    // An absent field must leave the current value alone.
    settings.inference_profiles = Some(vec![profile("coding", 0.2)]);
    settings.merge(&SettingsUpdate::default());
    assert!(settings.inference_profiles.is_some());
}

/// The repository stores one KV row per serde field and rebuilds
/// `Settings` from whatever rows exist, so an older row set (no profiles
/// row) must still deserialize.
#[test]
fn test_profiles_round_trip_through_json_and_default_when_absent() {
    let settings = Settings {
        inference_profiles: Some(vec![profile("coding", 0.2)]),
        ..Settings::with_defaults()
    };
    let value = serde_json::to_value(&settings).expect("serializes");
    assert!(value.get("inference_profiles").is_some());

    let restored: Settings = serde_json::from_value(value).expect("round-trips");
    assert_eq!(restored.inference_profiles, settings.inference_profiles);

    let absent: Settings = serde_json::from_str("{}").expect("deserializes without the field");
    assert_eq!(absent.inference_profiles, None);
}
