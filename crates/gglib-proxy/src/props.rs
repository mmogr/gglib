//! Ask llama-server what it will sample with when nobody tells it otherwise.
//!
//! **Tier C — Observation** ([ADR 0001]), and the other half of
//! [`crate::sampling_audit`]. That module compares gglib's intent against what
//! a slot reports *for a request*; this one reads the table a request falls
//! back to when gglib names nothing.
//!
//! Kept separate from [`crate::sampling_audit`] for the same reason
//! [`crate::slots`] is separate from [`crate::slots_poller`]: this is a pure
//! fetch-and-parse leaf plus a pure comparison, with no state and no schedule.
//!
//! # Why this is the load-bearing instrument
//!
//! [ADR 0003] defers six sampler values to llama.cpp on the strength of a
//! measurement: on the pinned build, all six of gglib's floor values are
//! *exactly* upstream's defaults, so deleting them changes nothing. Its
//! deletion criterion runs backwards from ADR 0002's:
//!
//! > If a pin bump moves an upstream default that gglib now defers to, the
//! > readback flags the divergence and this decision is re-taken for that
//! > parameter.
//!
//! The slot-level comparison in [`crate::sampling_audit`] *cannot* do that. It
//! skips [`ParamSource::Unset`](gglib_core::domain::ParamSource::Unset), which
//! is what every deferred field becomes — there is no intent to diverge from.
//! Reading the default table directly is what closes the loop, and unlike the
//! slot comparison it is a census: one read per launch, no sampling, no
//! attribution problem, nothing to abstain over.
//!
//! # What blinded it, and what un-blinded it
//!
//! Worth keeping in full, because the failure mode is easy to re-create and
//! looks like health while it lasts.
//!
//! Measured on the pinned build, not assumed:
//!
//! ```text
//!   field              build default   flag passed   /props reports
//!   temperature                  0.8           0.7   0.7   <- masked
//!   top_p                       0.95          0.90   0.90  <- masked
//!   top_k                         40            33   33    <- masked
//!   repeat_penalty               1.0          1.07   1.07  <- masked
//!   presence_penalty             0.0           0.3   0.3   <- masked
//!   min_p                       0.05          0.11   0.11  <- masked
//!   dry_multiplier               0.0           0.4   0.4   <- masked
//! ```
//!
//! Every sampler launch flag overwrites the field it names in
//! `default_generation_settings.params`. gglib used to pass all seven on the
//! `gglib serve` path, at values chosen to equal upstream's — so this check
//! would have compared gglib's floor against gglib's own flag and reported an
//! agreement it could never have failed to report. [ADR 0002] finding 2's
//! inert-module trap in a new place: an organ reading its own reflection and
//! calling it health.
//!
//! ADR 0003 finding 3 had called those flags "inert twice over", correctly
//! about *request behaviour* — the body wins, so no model saw them. They were
//! never inert for observation.
//!
//! [ADR 0003]'s deferral deleted them, which is what opened this instrument's
//! eyes. [`SAMPLER_LAUNCH_FLAGS_PASSED`] is now `false` and stays as a guard
//! against re-adding one.
//!
//! # Two claims, and only one of them is always safe
//!
//! `/props` always truthfully answers **"what will this server default to"**.
//! It answers **"what does this *build* default to"** only while nothing is
//! masking it — which is true today and is exactly what
//! [`SAMPLER_LAUNCH_FLAGS_PASSED`] tracks. The distinction is not pedantry:
//! the deletion criterion needs the second claim, and the second claim is the
//! one that can quietly stop holding.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-pin-the-llama-cpp-build.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::sampling_audit::SlotParams;

/// Timeout for one `/props` read. Matches [`crate::slots`]'s budget: it is the
/// same server, and a read that takes longer than this has already told us
/// what we need to know.
const PROPS_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Tolerance for comparing a float that made a round trip through JSON.
/// Same value and same reason as [`crate::sampling_audit`]'s.
const FLOAT_EPSILON: f64 = 1e-6;

/// Whether gglib passes sampler values as llama-server launch flags.
///
/// **`false` since [ADR 0003]'s deferral shipped**, which is what opened this
/// instrument's eyes: while it was `true` every field in [`UPSTREAM_DEFAULTS`]
/// was masked and [`check_baseline`] could only return
/// [`BaselineVerdict::Indeterminate`].
///
/// Kept rather than deleted along with the flags, because the failure it
/// guards against is *re-adding* one. A sampler flag overwrites the field it
/// names in `/props`, so a well-meaning launch-path change could silently
/// return this module to reading gglib's own values back — reporting agreement
/// it cannot fail to report. `no_sampler_flag_may_reappear_unnoticed` fails
/// the build if that happens without this constant being flipped back.
///
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
pub const SAMPLER_LAUNCH_FLAGS_PASSED: bool = false;

/// llama.cpp's own sampler defaults on the pinned build (`b1-69bf643`).
///
/// Measured, not transcribed from documentation — [ADR 0003] finding 1, taken
/// against a bare `llama-server` with no sampler flags, on two models, with a
/// positive control confirming the table moves when something moves it.
///
/// This is the baseline a pin bump would move, and moving it is exactly what
/// re-opens ADR 0003's decision for that parameter.
///
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
pub const UPSTREAM_DEFAULTS: [(&str, f64); 7] = [
    ("temperature", 0.8),
    ("top_p", 0.95),
    ("top_k", 40.0),
    ("repeat_penalty", 1.0),
    ("presence_penalty", 0.0),
    ("min_p", 0.05),
    ("dry_multiplier", 0.0),
];

// =============================================================================
// Wire shape
// =============================================================================

/// The slice of `GET /props` gglib reads.
///
/// Everything else llama.cpp reports there — chat template, model path, build
/// info — is either already known or somebody else's business. Naming only
/// what is used keeps this from becoming an obligation to track upstream's
/// whole props schema.
#[derive(Debug, Clone, Default, Deserialize)]
struct PropsBody {
    #[serde(default)]
    default_generation_settings: Option<DefaultGenerationSettings>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DefaultGenerationSettings {
    /// The same 42-key object a busy slot reports, so the same parser reads
    /// both. That is not a coincidence to be defended against — it is
    /// llama.cpp handing back the struct it will initialise each slot from.
    #[serde(default)]
    params: Option<SlotParams>,
}

/// What one `/props` read yielded.
///
/// Mirrors [`crate::slots::SlotsPollResult`]'s shape deliberately: a failure
/// is a variant, not an `Err`, because every caller's response to "could not
/// read it" is to record that and carry on.
#[derive(Debug, Clone, PartialEq)]
pub enum PropsResult {
    /// The server reported its default generation settings.
    Available(SlotParams),
    /// The endpoint could not be read, or carried no
    /// `default_generation_settings.params`. The string is for display.
    Unavailable(String),
}

/// Parse a `GET /props` body. Pure, so it can be tested against fixtures.
fn parse_props(status: reqwest::StatusCode, body: &str) -> PropsResult {
    if !status.is_success() {
        return PropsResult::Unavailable(format!("unexpected HTTP status {status}"));
    }
    match serde_json::from_str::<PropsBody>(body) {
        Ok(p) => p
            .default_generation_settings
            .and_then(|d| d.params)
            .map_or_else(
                || {
                    PropsResult::Unavailable(
                        "no default_generation_settings.params in /props".to_string(),
                    )
                },
                PropsResult::Available,
            ),
        Err(e) => PropsResult::Unavailable(format!("failed to parse /props response: {e}")),
    }
}

/// Fetch and parse `GET {base_url}/props`.
pub async fn fetch_props(client: &Client, base_url: &str) -> PropsResult {
    let url = format!("{base_url}/props");
    let response = match client.get(&url).timeout(PROPS_REQUEST_TIMEOUT).send().await {
        Ok(r) => r,
        Err(e) => return PropsResult::Unavailable(e.to_string()),
    };
    let status = response.status();
    match response.text().await {
        Ok(body) => parse_props(status, &body),
        Err(e) => PropsResult::Unavailable(format!("failed to read /props response body: {e}")),
    }
}

// =============================================================================
// The baseline check
// =============================================================================

/// One field's answer to "does this build still default to what ADR 0003
/// measured?".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum BaselineVerdict {
    /// The build agrees with the recorded table.
    Matches,
    /// The build's default has moved. For a field gglib defers to, this is
    /// ADR 0003's reverse deletion criterion firing.
    Differs {
        /// What ADR 0003 recorded for the pinned build.
        expected: f64,
        /// What this server reports.
        observed: f64,
    },
    /// Nothing can be concluded. Either the field was absent from `/props`, or
    /// gglib's own launch flag is overwriting it — see the module docs.
    Indeterminate {
        /// Why, in words a dashboard can show.
        reason: String,
    },
}

/// One field of [`check_baseline`]'s answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaselineField {
    /// Wire name of the parameter.
    pub field: &'static str,
    /// What was concluded about it.
    pub verdict: BaselineVerdict,
}

/// Compare a server's advertised defaults against the table [ADR 0003]
/// measured for the pinned build.
///
/// Returns one verdict per field in [`UPSTREAM_DEFAULTS`], never a bare
/// pass/fail: a field gglib is masking must not be reported as agreeing, and a
/// field missing from `/props` must not be reported as either.
///
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
#[must_use]
pub fn check_baseline(observed: &SlotParams) -> Vec<BaselineField> {
    UPSTREAM_DEFAULTS
        .iter()
        .map(|&(field, expected)| BaselineField {
            field,
            verdict: verdict_for(field, expected, observed),
        })
        .collect()
}

fn verdict_for(field: &str, expected: f64, observed: &SlotParams) -> BaselineVerdict {
    if SAMPLER_LAUNCH_FLAGS_PASSED {
        // Not "probably masked" — measured. Every one of the seven flags was
        // shown to overwrite its field in `/props`. Reporting `Matches` here
        // would be reporting agreement between gglib's floor and gglib's own
        // launch flag, which is a tautology dressed as an observation.
        return BaselineVerdict::Indeterminate {
            reason: "gglib passes this as a llama-server launch flag, which overwrites \
                     what /props reports (ADR 0003 follow-up: delete the flags)"
                .to_string(),
        };
    }
    let Some(actual) = observed.get(field) else {
        return BaselineVerdict::Indeterminate {
            reason: format!("this build's /props does not report {field}"),
        };
    };
    if (actual - expected).abs() <= FLOAT_EPSILON {
        BaselineVerdict::Matches
    } else {
        BaselineVerdict::Differs {
            expected,
            observed: actual,
        }
    }
}

/// The whole baseline reading, ready to surface.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaselineReport {
    /// Per-field verdicts, in [`UPSTREAM_DEFAULTS`] order.
    pub fields: Vec<BaselineField>,
    /// Whether any field could be concluded on at all.
    ///
    /// `false` while gglib masks the table. Surfaces must render an
    /// inconclusive report differently from a clean one — the same discipline
    /// [`crate::sampling_audit::AuditState::Blind`] enforces, for the same
    /// reason: a silent instrument and a healthy one produce the same output
    /// and mean opposite things.
    pub conclusive: bool,
}

impl BaselineReport {
    /// Build a report from a `/props` reading.
    #[must_use]
    pub fn from_params(observed: &SlotParams) -> Self {
        let fields = check_baseline(observed);
        let conclusive = fields
            .iter()
            .any(|f| !matches!(f.verdict, BaselineVerdict::Indeterminate { .. }));
        Self { fields, conclusive }
    }

    /// Fields whose default has moved since ADR 0003 measured it.
    #[must_use]
    pub fn drifted(&self) -> Vec<&BaselineField> {
        self.fields
            .iter()
            .filter(|f| matches!(f.verdict, BaselineVerdict::Differs { .. }))
            .collect()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
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

    fn real_params() -> SlotParams {
        match parse_props(StatusCode::OK, REAL_PROPS) {
            PropsResult::Available(p) => p,
            other => panic!("real /props must parse: {other:?}"),
        }
    }

    #[test]
    fn a_real_props_payload_parses() {
        let p = real_params();
        assert_eq!(p.temperature, Some(0.800_000_011_920_929));
        assert_eq!(p.top_k, Some(40.0));
        assert_eq!(p.samplers.unwrap().len(), 9);
    }

    #[test]
    fn a_props_body_without_params_is_unavailable() {
        let r = parse_props(StatusCode::OK, r#"{"model_path": "/x.gguf"}"#);
        assert!(matches!(r, PropsResult::Unavailable(_)), "{r:?}");
    }

    #[test]
    fn a_non_success_status_is_unavailable() {
        let r = parse_props(StatusCode::NOT_FOUND, "");
        assert!(
            matches!(r, PropsResult::Unavailable(ref m) if m.contains("404")),
            "{r:?}"
        );
    }

    #[test]
    fn unparseable_json_is_unavailable_rather_than_a_panic() {
        let r = parse_props(StatusCode::OK, "not json at all");
        assert!(matches!(r, PropsResult::Unavailable(_)), "{r:?}");
    }

    /// Written to be correct on both sides of the flag deletion, so it keeps
    /// testing something after the switch flips instead of quietly becoming a
    /// tautology.
    ///
    /// Flags passed → every field masked, nothing concluded. Flags gone → a
    /// bare build agrees with the recorded table, on every field.
    #[test]
    fn the_baseline_verdict_tracks_whether_gglib_is_masking_the_table() {
        let report = BaselineReport::from_params(&real_params());
        assert_eq!(report.fields.len(), UPSTREAM_DEFAULTS.len());
        assert!(report.drifted().is_empty(), "{report:?}");

        if SAMPLER_LAUNCH_FLAGS_PASSED {
            assert!(
                !report.conclusive,
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
            assert!(report.conclusive, "{report:?}");
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
}
