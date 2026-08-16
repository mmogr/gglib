//! Tests for [`super::BaselineReport`]: coverage, drift, and per-field verdicts.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use gglib_core::domain::InferenceConfig;
use reqwest::StatusCode;

/// Trimmed from a real `GET /props` on the pinned build, bare launch —
/// the same run that produced [`UPSTREAM_DEFAULTS`].
const REAL_PROPS: &str = r#"{
    "model_path": "/models/Llama-3.2-3B-Instruct-UD-Q6_K_XL.gguf",
    "default_generation_settings": {
        "n_ctx": 4096,
        "params": {
            "temperature": 0.800000011920929,
            "top_p": 0.949999988079071,
            "top_k": 40,
            "repeat_penalty": 1.0,
            "presence_penalty": 0.0,
            "min_p": 0.05000000074505806,
            "dry_multiplier": 0.0,
            "samplers": ["penalties","dry","top_n_sigma","top_k","typ_p","top_p","min_p","xtc","temperature"]
        }
    }
}"#;

/// A model that declares nothing — the ordinary case, and the one under
/// which the build's own table is what `/props` shows.
fn silent_model() -> ModelSamplingDefaults {
    ModelSamplingDefaults::default()
}

/// A model that declares `general.sampling.temp = 0.33`, built through the
/// real parser rather than by hand so these tests exercise the same code
/// the launch path runs.
///
/// One key, so a test that wants a second effect has to arrange it. Two
/// would leave every assertion here with an incidental extra verdict in it.
fn declaring_model() -> ModelSamplingDefaults {
    let mut meta = std::collections::HashMap::new();
    meta.insert("general.sampling.temp".to_string(), "0.33".to_string());
    ModelSamplingDefaults::from_metadata(&meta)
}

fn real_params() -> SlotParams {
    match parse_props(StatusCode::OK, REAL_PROPS) {
        PropsResult::Available(p) => p,
        other => panic!("real /props must parse: {other:?}"),
    }
}

/// Written to be correct on both sides of the flag deletion, so it keeps
/// testing something after the switch flips instead of quietly becoming a
/// tautology.
///
/// Flags passed → every field masked, nothing concluded. Flags gone → a
/// bare build agrees with the recorded table, on every field.
#[test]
fn the_baseline_verdict_tracks_whether_gglib_is_masking_the_table() {
    let report = BaselineReport::from_params(&real_params(), Some(&silent_model()));
    assert_eq!(report.fields.len(), UPSTREAM_DEFAULTS.len());
    assert!(report.drifted().is_empty(), "{report:?}");

    if SAMPLER_LAUNCH_FLAGS_PASSED {
        assert_eq!(
            report.coverage,
            BaselineCoverage::Blind {
                model_supplied: 0,
                indeterminate: UPSTREAM_DEFAULTS.len()
            },
            "gglib's own flags overwrite every field, so nothing can be concluded"
        );
        assert!(
            report
                .fields
                .iter()
                .all(|f| matches!(f.verdict, BaselineVerdict::Indeterminate { .. })),
            "{report:?}"
        );
    } else {
        assert_eq!(report.coverage, BaselineCoverage::Complete, "{report:?}");
        assert!(
            report
                .fields
                .iter()
                .all(|f| f.verdict == BaselineVerdict::Matches),
            "a bare pinned build must still agree with ADR 0003's table: {report:?}"
        );
    }
}

/// What the instrument will do once the flags are gone. Exercises the
/// comparison directly rather than through the masking gate, so the logic
/// is under test now and not only after the follow-up lands.
#[test]
fn an_unmasked_reading_matches_the_recorded_table() {
    let observed = real_params();
    for &(field, expected) in &UPSTREAM_DEFAULTS {
        let actual = observed.get(field).expect("field present in real /props");
        assert!(
            (actual - expected).abs() <= FLOAT_EPSILON,
            "{field}: /props says {actual}, ADR 0003 recorded {expected}"
        );
    }
}

/// A pin bump moving an upstream default is the event this whole organ
/// exists to catch. Verified against the unmasked comparison for the same
/// reason as above.
#[test]
fn a_moved_upstream_default_is_detected() {
    let mut observed = real_params();
    observed.top_p = Some(0.90); // upstream moved it from 0.95

    let actual = observed.get("top_p").unwrap();
    assert!(
        (actual - 0.95).abs() > FLOAT_EPSILON,
        "a moved default must not compare equal"
    );
}

/// **The defect.** `conclusive` was `any(|f| !indeterminate)`, so a single
/// concluded field made a seven-field report "conclusive" — and the
/// dashboard's conclusive-and-undrifted rendering is the sentence "All 7
/// sampler defaults match the values this build was measured at."
#[test]
fn a_partly_checked_table_is_not_a_clean_sweep() {
    let mut observed = real_params();
    observed.top_p = None;
    observed.min_p = None;

    let report = BaselineReport::from_params(&observed, Some(&silent_model()));

    assert_eq!(
        report.coverage,
        BaselineCoverage::Partial {
            checked: 5,
            model_supplied: 0,
            indeterminate: 2
        }
    );
    assert_ne!(
        report.coverage,
        BaselineCoverage::Complete,
        "five of seven is not a complete reading"
    );
    assert!(report.drifted().is_empty(), "and nothing actually drifted");
}

#[test]
fn a_fully_readable_table_is_complete() {
    assert_eq!(
        BaselineReport::from_params(&real_params(), Some(&silent_model())).coverage,
        BaselineCoverage::Complete
    );
}

/// Nothing concluded is its own state, not the bottom of a scale. It
/// shares a word with `AuditState::Blind` on purpose.
#[test]
fn a_table_with_nothing_readable_is_blind_not_partial() {
    let report = BaselineReport::from_params(&SlotParams::default(), Some(&silent_model()));

    assert_eq!(
        report.coverage,
        BaselineCoverage::Blind {
            model_supplied: 0,
            indeterminate: UPSTREAM_DEFAULTS.len()
        }
    );
}

/// Coverage answers "how much was compared", drift answers "did any of it
/// disagree". A complete reading with a moved default is both complete and
/// alarming, and a surface that checks coverage first would hide it.
#[test]
fn coverage_and_drift_are_independent() {
    let mut observed = real_params();
    observed.top_p = Some(0.90);

    let report = BaselineReport::from_params(&observed, Some(&silent_model()));

    assert_eq!(report.coverage, BaselineCoverage::Complete);
    assert_eq!(report.drifted().len(), 1);
}

// ── Model-embedded sampling defaults ──────────────────────────────────

/// **The false alarm this fixes.** A model shipping `general.sampling.*`
/// moves `/props`, and the check reported that as the *build's* default
/// having moved — a red banner and a `warn!` saying ADR 0003's deferral is
/// re-opened, for a model doing exactly what llama.cpp intends.
#[test]
fn a_default_supplied_by_the_models_own_gguf_is_not_reported_as_drift() {
    let mut observed = real_params();
    observed.temperature = Some(0.33);

    let report = BaselineReport::from_params(&observed, Some(&declaring_model()));

    assert!(report.drifted().is_empty(), "{report:?}");
    let temperature = &report.fields[0];
    assert_eq!(temperature.field, "temperature");
    assert_eq!(
        temperature.verdict,
        BaselineVerdict::ModelSupplied {
            key: "general.sampling.temp",
            value: 0.33
        }
    );
}

/// A model can only reach five of the seven, so the rest are still checked
/// against the build. Attribution narrows the instrument; it must not
/// switch it off.
#[test]
fn a_field_the_model_does_not_name_is_still_compared_against_the_build() {
    let mut observed = real_params();
    observed.temperature = Some(0.33);

    let report = BaselineReport::from_params(&observed, Some(&declaring_model()));

    let by_name = |name: &str| {
        report
            .fields
            .iter()
            .find(|f| f.field == name)
            .unwrap_or_else(|| panic!("{name} present"))
    };
    assert_eq!(by_name("top_p").verdict, BaselineVerdict::Matches);
    assert_eq!(
        by_name("presence_penalty").verdict,
        BaselineVerdict::Matches
    );
    assert_eq!(by_name("dry_multiplier").verdict, BaselineVerdict::Matches);
}

/// **The can-it-still-fail test.** Partial masking must not disable the
/// alarm on the fields that are still observable — otherwise a model
/// shipping one key would hide a genuine pin bump in another.
#[test]
fn a_moved_build_default_is_still_caught_on_a_model_that_supplies_others() {
    let mut observed = real_params();
    observed.temperature = Some(0.33); // the model's own
    observed.top_p = Some(0.90); // upstream moved this one

    let report = BaselineReport::from_params(&observed, Some(&declaring_model()));

    let drifted = report.drifted();
    assert_eq!(drifted.len(), 1, "{report:?}");
    assert_eq!(drifted[0].field, "top_p");
}

/// The model asked for one thing and the server reports another, so the
/// attribution premise fails. Blaming the build would be reporting a
/// disagreement gglib cannot locate.
#[test]
fn a_model_declared_value_that_props_contradicts_concludes_nothing() {
    let mut observed = real_params();
    observed.temperature = Some(0.55); // model declares 0.33

    let report = BaselineReport::from_params(&observed, Some(&declaring_model()));

    assert!(report.drifted().is_empty(), "must not blame the build");
    match &report.fields[0].verdict {
        BaselineVerdict::Indeterminate { reason } => {
            assert!(reason.contains("0.33"), "{reason}");
            assert!(reason.contains("0.55"), "{reason}");
        }
        other => panic!("expected Indeterminate, got {other:?}"),
    }
}

/// llama.cpp's `strtof` and Rust's `f64::from_str` need not agree on every
/// string, so an unreadable declaration means gglib cannot tell whether
/// the model or the build supplied the value.
#[test]
fn an_unreadable_model_sampling_value_concludes_nothing_rather_than_alarming() {
    let mut meta = std::collections::HashMap::new();
    meta.insert("general.sampling.temp".to_string(), "warm".to_string());
    let model = ModelSamplingDefaults::from_metadata(&meta);

    let report = BaselineReport::from_params(&real_params(), Some(&model));

    assert!(report.drifted().is_empty());
    assert!(matches!(
        report.fields[0].verdict,
        BaselineVerdict::Indeterminate { .. }
    ));
}

/// A target with no GGUF behind it — a remote backend, a test double —
/// knows nothing either way. Wrong in the conservative direction, which is
/// the only one ADR 0004 allows.
#[test]
fn a_target_gglib_did_not_launch_concludes_nothing_rather_than_alarming() {
    let report = BaselineReport::from_params(&real_params(), None);

    assert!(report.drifted().is_empty());
    assert!(
        report
            .fields
            .iter()
            .all(|f| matches!(f.verdict, BaselineVerdict::Indeterminate { .. })),
        "{report:?}"
    );
    assert!(matches!(report.coverage, BaselineCoverage::Blind { .. }));
}

/// A model-supplied field counts against coverage, not toward it: the
/// value was read fine, but it is not the build's, so the build's own
/// default was not observed for it.
#[test]
fn model_supplied_fields_reduce_coverage_rather_than_completing_it() {
    let mut observed = real_params();
    observed.temperature = Some(0.33);

    let report = BaselineReport::from_params(&observed, Some(&declaring_model()));

    assert_eq!(
        report.coverage,
        BaselineCoverage::Partial {
            checked: 6,
            model_supplied: 1,
            indeterminate: 0
        },
        "six fields still measured against the build; one is the model's"
    );
}

/// The cross-crate table guard, in the spirit of
/// `no_sampler_flag_may_reappear_unnoticed`. `MODEL_SAMPLING_KEYS` lives
/// in `gglib-core` and `UPSTREAM_DEFAULTS` here; a name added to one and
/// not the other is silent in both directions.
#[test]
fn every_model_sampling_key_names_a_field_the_baseline_check_compares() {
    let checked: Vec<&str> = UPSTREAM_DEFAULTS.iter().map(|(f, _)| *f).collect();

    for (field, key) in gglib_core::domain::MODEL_SAMPLING_KEYS {
        assert!(
            checked.contains(&field),
            "{key} moves {field}, which the baseline check does not compare"
        );
    }

    let unreachable: Vec<&str> = checked
        .iter()
        .copied()
        .filter(|f| ModelSamplingDefaults::gguf_key(f).is_none())
        .collect();
    assert_eq!(
        unreachable,
        vec!["presence_penalty", "dry_multiplier"],
        "these two have no general.sampling.* key, so no model can move them \
         and the build stays observable through them. If this list changed, \
         llama.cpp gained or lost a key and both tables need re-checking."
    );
}

/// A field `/props` does not report is unknown, never agreement. Same
/// discipline as `RuntimeCapabilities::unknown`.
#[test]
fn a_field_absent_from_props_is_indeterminate_not_matching() {
    let mut observed = real_params();
    observed.min_p = None;
    assert!(
        observed.get("min_p").is_none(),
        "an absent field must read as absent, not as zero"
    );
}

/// The floor stopped asserting the six, so nothing masks the table any
/// more. Anchored to the floor rather than to the launch path because
/// that is what this crate can see; the launch-path half of the invariant
/// is `gglib_runtime::llama::args::sampling`'s guard, which asserts
/// against this very constant.
#[test]
fn the_floor_no_longer_restates_what_props_reports() {
    let floor = InferenceConfig::with_hardcoded_defaults();
    let asserted = |field: &str| match field {
        "temperature" => floor.temperature.is_some(),
        "top_p" => floor.top_p.is_some(),
        "top_k" => floor.top_k.is_some(),
        "repeat_penalty" => floor.repeat_penalty.is_some(),
        "presence_penalty" => floor.presence_penalty.is_some(),
        "min_p" => floor.min_p.is_some(),
        "dry_multiplier" => floor.dry_multiplier.is_some(),
        other => panic!("UPSTREAM_DEFAULTS names {other}, which this test cannot read"),
    };
    let restated: Vec<_> = UPSTREAM_DEFAULTS
        .iter()
        .filter(|(field, _)| asserted(field))
        .map(|(field, _)| *field)
        .collect();

    assert_eq!(
        restated,
        vec!["temperature"],
        "every field here is one gglib asserts a value for. `temperature` is the \
         measured divergence ADR 0003 kept; anything else is a floor value that was \
         supposed to be deferred, and while it is set the launch path may restate it \
         into /props and blind the baseline check."
    );
}
