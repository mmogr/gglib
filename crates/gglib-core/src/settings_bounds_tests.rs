//! Tests for the bounds in [`super`]: each is the number written out here,
//! each validator holds its setting to it, and
//! `contracts/settings/bounds.json` is what the validators enforce.
//!
//! That file is the frontend's side of the bounds.
//! `tests/ts/contracts/settingsBounds.test.ts` holds the settings page's
//! ranges and defaults to it, and reads no Rust to do so.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Value, json};

use super::*;
use crate::domain::{FieldIssue, InferenceConfig};
use crate::settings::{Settings, SettingsError, validate_inference_config, validate_settings};

/// What a validator accepts for one number: `min` and up, or anything greater
/// than `above`, and no further than `max` where there is a ceiling.
#[derive(Serialize)]
struct Accepted {
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    above: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<Value>,
}

fn within<T: Copy + Into<Value>>(range: &RangeInclusive<T>) -> Accepted {
    Accepted {
        min: Some((*range.start()).into()),
        above: None,
        max: Some((*range.end()).into()),
    }
}

fn at_least(min: impl Into<Value>) -> Accepted {
    Accepted {
        min: Some(min.into()),
        above: None,
        max: None,
    }
}

fn above(exclusive_min: impl Into<Value>) -> Accepted {
    Accepted {
        min: None,
        above: Some(exclusive_min.into()),
        max: None,
    }
}

/// `contracts/settings/bounds.json`: the range each validator holds a number
/// to, under the name the number is stored by, and the defaults and floors the
/// settings page states.
#[derive(Serialize)]
struct Bounds {
    settings: BTreeMap<&'static str, Accepted>,
    settings_defaults: BTreeMap<&'static str, Value>,
    inference: BTreeMap<&'static str, Accepted>,
    inference_floor: InferenceConfig,
    reasoning_floor: InferenceConfig,
}

fn bounds() -> Bounds {
    let defaults = serde_json::to_value(Settings::with_defaults()).expect("serialise");
    let default_of = |field: &'static str| {
        let value = defaults.get(field).expect("a field of Settings");
        (field, value.clone())
    };
    Bounds {
        settings: BTreeMap::from([
            ("default_context_size", within(&CONTEXT_SIZE_RANGE)),
            ("proxy_port", at_least(MIN_PORT)),
            ("llama_base_port", at_least(MIN_PORT)),
            ("max_download_queue_size", within(&DOWNLOAD_QUEUE_RANGE)),
        ]),
        settings_defaults: BTreeMap::from([
            default_of("default_context_size"),
            default_of("proxy_port"),
            default_of("llama_base_port"),
            default_of("max_download_queue_size"),
            default_of("max_tool_iterations"),
            default_of("default_image_model_id"),
            default_of("mcp_drawing"),
        ]),
        inference: BTreeMap::from([
            ("temperature", within(&TEMPERATURE_RANGE)),
            ("topP", within(&TOP_P_RANGE)),
            ("topK", at_least(TOP_K_MIN)),
            ("maxTokens", at_least(MAX_TOKENS_MIN)),
            (
                "reasoningBudgetTokens",
                at_least(REASONING_BUDGET_TOKENS_MIN),
            ),
            ("repeatPenalty", above(REPEAT_PENALTY_EXCLUSIVE_MIN)),
            ("presencePenalty", within(&PRESENCE_PENALTY_RANGE)),
            ("minP", within(&MIN_P_RANGE)),
            ("frequencyPenalty", within(&FREQUENCY_PENALTY_RANGE)),
            ("dynatempRange", at_least(DYNATEMP_RANGE_MIN)),
            ("dynatempExponent", above(DYNATEMP_EXPONENT_EXCLUSIVE_MIN)),
            ("topNSigma", at_least(TOP_N_SIGMA_MIN)),
            ("dryMultiplier", within(&DRY_MULTIPLIER_RANGE)),
            ("dryBase", above(DRY_BASE_EXCLUSIVE_MIN)),
            ("dryAllowedLength", at_least(DRY_ALLOWED_LENGTH_MIN)),
            ("dryPenaltyLastN", at_least(DRY_PENALTY_LAST_N_MIN)),
        ]),
        inference_floor: InferenceConfig::with_hardcoded_defaults(),
        reasoning_floor: InferenceConfig::reasoning_floor(),
    }
}

/// The same ranges as numbers, so no bound moves by accident: one changed in
/// `settings_bounds.rs` fails here until it is changed here too.
fn written_out() -> Value {
    json!({
        "settings": {
            "default_context_size": {"min": 512, "max": 1_000_000},
            "llama_base_port": {"min": 1024},
            "max_download_queue_size": {"min": 1, "max": 50},
            "proxy_port": {"min": 1024},
        },
        "inference": {
            "temperature": {"min": 0.0, "max": 2.0},
            "topP": {"min": 0.0, "max": 1.0},
            "topK": {"min": 1},
            "maxTokens": {"min": 1},
            "reasoningBudgetTokens": {"min": -1},
            "repeatPenalty": {"above": 0.0},
            "presencePenalty": {"min": 0.0, "max": 2.0},
            "minP": {"min": 0.0, "max": 1.0},
            "frequencyPenalty": {"min": -2.0, "max": 2.0},
            "dynatempRange": {"min": 0.0},
            "dynatempExponent": {"above": 0.0},
            "topNSigma": {"min": -1.0},
            "dryMultiplier": {"min": 0.0, "max": 5.0},
            "dryBase": {"above": 1.0},
            "dryAllowedLength": {"min": 0},
            "dryPenaltyLastN": {"min": -1},
        },
    })
}

#[test]
fn each_bound_is_the_number_written_out_here() {
    let named = serde_json::to_value(bounds()).expect("serialise");
    let written = written_out();
    assert_eq!(named["settings"], written["settings"]);
    assert_eq!(named["inference"], written["inference"]);
}

/// A bound moved by `steps`: a whole number by ones, a fraction by hundredths.
fn stepped(edge: &Value, steps: i32) -> Value {
    edge.as_i64().map_or_else(
        || json!(edge.as_f64().expect("a number") + f64::from(steps) / 100.0),
        |whole| json!(whole + i64::from(steps)),
    )
}

/// Whether `value`, stored alone under `field`, passes its section's validator.
fn stores(section: &str, field: &str, value: &Value) -> bool {
    let stored = json!({ field: value });
    if section == "settings" {
        let settings: Settings = serde_json::from_value(stored).expect("settings");
        validate_settings(&settings).is_ok()
    } else {
        let config: InferenceConfig = serde_json::from_value(stored).expect("a config");
        validate_inference_config(&config).is_ok()
    }
}

/// Both ends of a range are accepted and one step past either is refused; an
/// exclusive floor is itself refused, and a range with no ceiling accepts the
/// largest number every bounded field's type holds.
#[test]
fn each_validator_accepts_its_range_and_refuses_one_step_outside() {
    let written = written_out();
    for section in ["settings", "inference"] {
        for (field, range) in written[section].as_object().expect("a section") {
            let stores = |value: &Value| stores(section, field, value);
            match (range.get("min"), range.get("above")) {
                (Some(min), None) => {
                    assert!(stores(min), "{field} must accept {min}");
                    let below = stepped(min, -1);
                    assert!(!stores(&below), "{field} must refuse {below}");
                }
                (None, Some(above)) => {
                    assert!(!stores(above), "{field} must refuse {above}");
                    let inside = stepped(above, 1);
                    assert!(stores(&inside), "{field} must accept {inside}");
                }
                _ => panic!("{field} needs a `min` or an `above`, and not both"),
            }
            if let Some(max) = range.get("max") {
                assert!(stores(max), "{field} must accept {max}");
                let past = stepped(max, 1);
                assert!(!stores(&past), "{field} must refuse {past}");
            } else {
                assert!(stores(&json!(u16::MAX)), "{field} has no ceiling");
            }
        }
    }
}

/// What a refused setting is told, in full. The numbers are formatted from
/// the constants; they are written out here as a person reads them.
#[test]
fn a_refused_setting_is_told_its_bound() {
    for (error, message) in [
        (
            SettingsError::InvalidContextSize(100),
            "Context size must be between 512 and 1000000, got 100",
        ),
        (
            SettingsError::InvalidPort(80),
            "Port should be >= 1024 (privileged ports require root), got 80",
        ),
        (
            SettingsError::InvalidQueueSize(0),
            "Max download queue size must be between 1 and 50, got 0",
        ),
    ] {
        assert_eq!(error.to_string(), message);
    }
}

/// A budget on a request is held to the floor a stored one is, and the
/// refusal names that floor.
#[test]
fn a_requests_reasoning_budget_is_held_to_the_stored_floor() {
    let read = |budget: i32| {
        InferenceConfig::extract_client_sampling(&json!({"reasoning_budget_tokens": budget}))
    };

    let (config, issues) = read(REASONING_BUDGET_TOKENS_MIN);
    assert_eq!(
        config.reasoning_budget_tokens,
        Some(REASONING_BUDGET_TOKENS_MIN)
    );
    assert!(issues.is_empty(), "the floor itself is read: {issues:?}");

    let (config, issues) = read(REASONING_BUDGET_TOKENS_MIN - 1);
    assert_eq!(config.reasoning_budget_tokens, None);
    let [FieldIssue::Rejected { expected, .. }] = issues.as_slice() else {
        panic!("one step below the floor is refused: {issues:?}");
    };
    assert!(
        expected.contains(&format!(">= {REASONING_BUDGET_TOKENS_MIN} ")),
        "the refusal must name the floor: {expected}"
    );
}

/// The agent's limits are clamped where a loop reads them and never refused
/// at save, so the file gives them no range and the settings page's caps on
/// them are its own.
#[test]
fn the_agent_limits_are_stored_whatever_their_size() {
    for field in ["max_tool_iterations", "max_stagnation_steps"] {
        assert!(!bounds().settings.contains_key(field), "{field}");
        for size in [0, u32::MAX] {
            assert!(
                stores("settings", field, &json!(size)),
                "{field} must store {size}"
            );
        }
    }
}

fn bounds_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/settings/bounds.json")
}

/// The checked-in file is exactly what the constants and the floors give. Run
/// with `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
#[test]
fn the_checked_in_bounds_are_the_ones_the_validators_use() {
    let mut want = serde_json::to_string_pretty(&bounds()).expect("serialise");
    want.push('\n');
    let path = bounds_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("make contracts/settings");
        std::fs::write(&path, &want).expect("write bounds.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/settings/bounds.json");
    assert!(
        have == want,
        "contracts/settings/bounds.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}
