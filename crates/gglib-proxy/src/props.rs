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
//! masking it. The distinction is not pedantry: the deletion criterion needs
//! the second claim, and the second claim is the one that can quietly stop
//! holding.
//!
//! # Three things can mask it, not one
//!
//! [`SAMPLER_LAUNCH_FLAGS_PASSED`] tracks the first and was for a while
//! presented as the whole story. It is not.
//!
//! 1. **A gglib launch flag.** Measured above. `false` since ADR 0003.
//! 2. **The model's own GGUF.** llama.cpp PR #17120 — in the pinned build —
//!    added `common_init_sampler_from_model`, which overwrites
//!    `params.sampling` from `general.sampling.*` for every field no CLI flag
//!    set, and this endpoint is rendered from that struct. Five of the seven
//!    fields here can be moved that way; `presence_penalty` and
//!    `dry_multiplier` have no GGUF key and cannot. See
//!    [`ModelSamplingDefaults`].
//! 3. **Not having launched the server.** A target gglib did not spawn has no
//!    catalog row behind it, so nothing is known about what its model
//!    declares and no field can be attributed.
//!
//! The second is why the check takes the model's declarations as an argument
//! rather than comparing `/props` against a per-build constant alone. Without
//! it, a model shipping its author's recommended temperature reports as *the
//! build's default having moved* — ADR 0003's deferral re-opened, in red, for
//! a model doing exactly what llama.cpp intends.
//!
//! Note what this does **not** do: it never compares `/props` against the
//! model's value and calls agreement a match. The observed value *is* the
//! model's value by construction, so such a comparison could not fail — the
//! same inert-check trap as the launch flags, in a third guise. A
//! model-supplied field gets its own verdict and counts against coverage.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-pin-the-llama-cpp-build.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use gglib_core::domain::{ModelSamplingDefault, ModelSamplingDefaults, TemplateCaps};

use crate::sampling_audit::SlotParams;
use crate::template_caps_read::PropsReading;

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
    /// The loaded template's capability self-report (ADR 0007). Independent
    /// of the sampler table above — see [`crate::template_caps_read`].
    #[serde(default)]
    chat_template_caps: Option<TemplateCaps>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DefaultGenerationSettings {
    /// The same 42-key object a busy slot reports, so the same parser reads
    /// both. That is not a coincidence to be defended against — it is
    /// llama.cpp handing back the struct it will initialise each slot from.
    #[serde(default)]
    params: Option<SlotParams>,
}

/// What one `/props` read yielded for the default sampler table — one half of
/// a [`PropsReading`], beside the template-caps half it used to swallow.
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
fn parse_props(status: reqwest::StatusCode, body: &str) -> PropsReading {
    if !status.is_success() {
        return PropsReading::unreadable(format!("unexpected HTTP status {status}"));
    }
    match serde_json::from_str::<PropsBody>(body) {
        Ok(p) => PropsReading::of(
            p.default_generation_settings.and_then(|d| d.params),
            p.chat_template_caps,
        ),
        Err(e) => PropsReading::unreadable(format!("failed to parse /props response: {e}")),
    }
}

/// Fetch and parse `GET {base_url}/props`, yielding both of its halves.
pub async fn fetch_props(client: &Client, base_url: &str) -> PropsReading {
    let url = format!("{base_url}/props");
    let response = match client.get(&url).timeout(PROPS_REQUEST_TIMEOUT).send().await {
        Ok(r) => r,
        Err(e) => return PropsReading::unreadable(e.to_string()),
    };
    let status = response.status();
    match response.text().await {
        Ok(body) => parse_props(status, &body),
        Err(e) => PropsReading::unreadable(format!("failed to read /props response body: {e}")),
    }
}

// =============================================================================
// The baseline check
// =============================================================================

/// One field's answer to "does this build still default to what ADR 0003
/// measured?".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    /// The effective default came from **this model's own GGUF**, not the
    /// build.
    ///
    /// Not agreement and not drift — a third answer, and since llama.cpp PR
    /// #17120 the common one. `common_init_sampler_from_model` overwrites
    /// `params.sampling` from `general.sampling.*` for every field no CLI flag
    /// sets, and `/props` is rendered from that struct, so the number here is
    /// the model's, faithfully reported.
    ///
    /// [ADR 0003]'s reverse deletion criterion is neither fired nor satisfied
    /// by this: a model moving a value says nothing about whether a pin bump
    /// moved the value gglib defers to. What it does mean is that **the
    /// build's own default is unobservable on this launch for this field**,
    /// which is why it counts toward coverage rather than toward agreement.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    ModelSupplied {
        /// The GGUF key that moved it, e.g. `general.sampling.temp`.
        key: &'static str,
        /// What the model asked for, and what `/props` confirms it got.
        value: f64,
    },
    /// Nothing can be concluded. The field was absent from `/props`, or
    /// something is masking the build's own value — see the module docs.
    Indeterminate {
        /// Why, in words a dashboard can show.
        reason: String,
    },
}

/// One field of [`check_baseline`]'s answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
pub fn check_baseline(
    observed: &SlotParams,
    model: Option<&ModelSamplingDefaults>,
) -> Vec<BaselineField> {
    UPSTREAM_DEFAULTS
        .iter()
        .map(|&(field, expected)| BaselineField {
            field,
            verdict: verdict_for(field, expected, observed, model),
        })
        .collect()
}

fn verdict_for(
    field: &'static str,
    expected: f64,
    observed: &SlotParams,
    model: Option<&ModelSamplingDefaults>,
) -> BaselineVerdict {
    if SAMPLER_LAUNCH_FLAGS_PASSED {
        // Not "probably masked" — measured. Every one of the seven flags was
        // shown to overwrite its field in `/props`. Reporting `Matches` here
        // would be reporting agreement between gglib's floor and gglib's own
        // launch flag, which is a tautology dressed as an observation. A flag
        // also beats the model's GGUF, which is why this stays first.
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

    let Some(model) = model else {
        return BaselineVerdict::Indeterminate {
            reason: "gglib did not launch this instance, so it has no GGUF metadata to \
                     attribute this against"
                .to_string(),
        };
    };

    match model.get(field) {
        ModelSamplingDefault::Unreadable => {
            let key = ModelSamplingDefaults::gguf_key(field).unwrap_or("general.sampling.*");
            BaselineVerdict::Indeterminate {
                reason: format!(
                    "this model declares {key} and gglib could not read it as a number, \
                     so {field} cannot be attributed to the build or to the model"
                ),
            }
        }
        ModelSamplingDefault::Declared(declared) => {
            let key = ModelSamplingDefaults::gguf_key(field).unwrap_or("general.sampling.*");
            if (declared - actual).abs() <= FLOAT_EPSILON {
                BaselineVerdict::ModelSupplied { key, value: actual }
            } else {
                // The model asked for one thing and the server reports
                // another, so the attribution premise fails here. Not
                // `Differs`: that would blame the build for a disagreement
                // gglib cannot locate. This arm is also a free positive
                // control on the model-metadata path itself.
                BaselineVerdict::Indeterminate {
                    reason: format!(
                        "this model's {key} declares {declared} but /props reports {actual}; \
                         gglib cannot say which supplied it"
                    ),
                }
            }
        }
        ModelSamplingDefault::Absent => {
            if (actual - expected).abs() <= FLOAT_EPSILON {
                BaselineVerdict::Matches
            } else {
                BaselineVerdict::Differs {
                    expected,
                    observed: actual,
                }
            }
        }
    }
}

/// How much of the table a reading actually covered.
///
/// A tagged union rather than the `conclusive: bool` it replaced, and the bool
/// is worth describing because of how it failed. It was computed as *"any
/// field reached a verdict"*, so a report in which two of seven fields were
/// checked and five could not be reported itself as conclusive — and the
/// dashboard's only conclusive-and-undrifted rendering is the sentence "All 7
/// sampler defaults match the values this build was measured at."
///
/// That is [`AuditState`](crate::sampling_audit::AuditState)'s failure one
/// level up: not a field rendered as agreeing when it was unknown, but a
/// *report* rendered as complete when it was partial. The same rule applies
/// and the same remedy — make the state carry what it knows instead of
/// collapsing it to a yes/no.
///
/// Orthogonal to whether anything drifted. [`Complete`](Self::Complete) says
/// every field was compared, not that every field agreed, so surfaces check
/// [`BaselineReport::drifted`] first and coverage second.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "coverage", rename_all = "snake_case")]
pub enum BaselineCoverage {
    /// Every field in [`UPSTREAM_DEFAULTS`] was compared against the recorded
    /// table. The only state in which an all-clear may be rendered.
    Complete,
    /// Some fields were compared and some could not be.
    Partial {
        /// Fields compared against the recorded table.
        checked: usize,
        /// Fields whose value came from the model's own GGUF, so the build's
        /// default is unobservable for them on this launch.
        model_supplied: usize,
        /// Fields nothing could be concluded about.
        indeterminate: usize,
    },
    /// No field was compared at all. Deliberately shares a word with
    /// [`AuditState::Blind`](crate::sampling_audit::AuditState::Blind): same
    /// meaning, same discipline, and the parallel between this organ's two
    /// halves is worth being able to see.
    Blind {
        /// Fields whose value came from the model's own GGUF.
        model_supplied: usize,
        /// Fields nothing could be concluded about.
        indeterminate: usize,
    },
}

impl BaselineCoverage {
    /// Classify a set of per-field verdicts.
    ///
    /// `ModelSupplied` counts against coverage rather than toward it: the
    /// field was read successfully and nothing is wrong, but what was read is
    /// the model's number, so the build's own default was not observed.
    fn of(fields: &[BaselineField]) -> Self {
        let indeterminate = fields
            .iter()
            .filter(|f| matches!(f.verdict, BaselineVerdict::Indeterminate { .. }))
            .count();
        let model_supplied = fields
            .iter()
            .filter(|f| matches!(f.verdict, BaselineVerdict::ModelSupplied { .. }))
            .count();
        let checked = fields.len() - indeterminate - model_supplied;

        // An empty table is not a clean sweep over nothing. Unreachable while
        // `UPSTREAM_DEFAULTS` is a fixed array, and spelled out anyway because
        // "zero problems found" is exactly the answer this type exists to stop
        // being ambiguous.
        if fields.is_empty() {
            return Self::Blind {
                model_supplied: 0,
                indeterminate: 0,
            };
        }
        if checked == fields.len() {
            Self::Complete
        } else if checked == 0 {
            Self::Blind {
                model_supplied,
                indeterminate,
            }
        } else {
            Self::Partial {
                checked,
                model_supplied,
                indeterminate,
            }
        }
    }
}

/// The whole baseline reading, ready to surface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct BaselineReport {
    /// Per-field verdicts, in [`UPSTREAM_DEFAULTS`] order.
    pub fields: Vec<BaselineField>,
    /// How much of the table this reading covered. See [`BaselineCoverage`].
    pub coverage: BaselineCoverage,
}

impl BaselineReport {
    /// Build a report from a `/props` reading.
    #[must_use]
    pub fn from_params(observed: &SlotParams, model: Option<&ModelSamplingDefaults>) -> Self {
        let fields = check_baseline(observed, model);
        let coverage = BaselineCoverage::of(&fields);
        Self { fields, coverage }
    }

    /// Fields whose value came from the model's own GGUF rather than the build.
    #[must_use]
    pub fn model_supplied(&self) -> Vec<&BaselineField> {
        self.fields
            .iter()
            .filter(|f| matches!(f.verdict, BaselineVerdict::ModelSupplied { .. }))
            .collect()
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

/// What the `/props` baseline read has produced for the running model.
///
/// A tagged union rather than `Option<BaselineReport>`, for
/// [`AuditState`](crate::sampling_audit::AuditState)'s reason. "Nobody has
/// read it yet" and "the read was attempted and failed, and here is why" are
/// different facts, and an `Option` flattens both into the same `None` — after
/// which the only thing a surface can say is "not read yet", which is a claim
/// about a read that did happen.
///
/// That is the blind-rendered-as-health collapse this subsystem exists to
/// prevent, one level down from where it was being prevented: the slot half
/// carried `Blind { reason }` from the start, and the baseline half did not.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BaselineState {
    /// No `/props` read has completed for the running model yet. The ordinary
    /// state for the first second of a launch.
    #[default]
    NotYetRead,
    /// The read was attempted and did not produce a table.
    ///
    /// Usually a server that has not finished starting, which is why the
    /// poller retries rather than latching — see `slots_poller`'s
    /// `BaselineLatch`.
    Unreadable {
        /// Cause, in words a dashboard can show.
        reason: String,
    },
    /// The build's default table was read.
    Read {
        /// Per-field verdicts.
        report: BaselineReport,
    },
}

impl BaselineState {
    /// The report, when one was read.
    #[must_use]
    pub const fn report(&self) -> Option<&BaselineReport> {
        match self {
            Self::Read { report } => Some(report),
            Self::NotYetRead | Self::Unreadable { .. } => None,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "props_parse_tests.rs"]
mod props_parse_tests;

#[cfg(test)]
#[path = "props_baseline_tests.rs"]
mod props_baseline_tests;
