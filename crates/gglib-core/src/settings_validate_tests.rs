//! Tests for the inference validators' wording. Which values they accept is
//! tested with the bounds themselves, in `settings_bounds_tests.rs`.

use serde_json::json;

use super::validate_inference_config;
use crate::domain::InferenceConfig;

/// What each refusal says, in full. The numbers in it are formatted from the
/// constants in `settings_bounds.rs`; they are written out here as a person
/// reads them.
#[test]
fn each_refusal_quotes_its_bound() {
    for (stored, message) in [
        (
            json!({"temperature": 2.5}),
            "Temperature must be between 0.0 and 2.0, got 2.5",
        ),
        (
            json!({"topP": 1.5}),
            "Top P must be between 0.0 and 1.0, got 1.5",
        ),
        (json!({"topK": 0}), "Top K must be 1 or greater, got 0"),
        (json!({"maxTokens": 0}), "Max tokens must be 1 or greater"),
        (
            json!({"reasoningBudgetTokens": -2}),
            "Reasoning budget tokens must be -1 or greater \
             (-1 defers to the launch default, 0 stops thinking), got -2",
        ),
        (
            json!({"repeatPenalty": 0.0}),
            "Repeat penalty must be greater than 0.0, got 0",
        ),
        (
            json!({"presencePenalty": 2.5}),
            "Presence penalty must be between 0.0 and 2.0, got 2.5",
        ),
        (
            json!({"minP": 1.5}),
            "Min P must be between 0.0 and 1.0, got 1.5",
        ),
        (
            json!({"frequencyPenalty": -2.5}),
            "Frequency penalty must be between -2.0 and 2.0, got -2.5",
        ),
        (
            json!({"dynatempRange": -0.5}),
            "Dynatemp range must be 0.0 or greater (0.0 disables), got -0.5",
        ),
        (
            json!({"dynatempExponent": 0.0}),
            "Dynatemp exponent must be greater than 0.0, got 0",
        ),
        (
            json!({"topNSigma": -1.5}),
            "Top-n-sigma must be -1.0 (disabled) or greater, got -1.5",
        ),
        (
            json!({"dryMultiplier": 5.5}),
            "DRY multiplier must be between 0.0 and 5.0, got 5.5",
        ),
        (
            json!({"dryBase": 1.0}),
            "DRY base must be greater than 1.0, got 1",
        ),
        (
            json!({"dryAllowedLength": -1}),
            "DRY allowed length must be 0 or greater, got -1",
        ),
        (
            json!({"dryPenaltyLastN": -2}),
            "DRY penalty last N must be -1 or greater (0 disables), got -2",
        ),
    ] {
        let config: InferenceConfig = serde_json::from_value(stored).expect("a config");
        let said = validate_inference_config(&config).expect_err("refused");
        assert_eq!(said, message);
    }
}
