//! Inference configuration types.
//!
//! Defines shared types for configuring LLM inference parameters
//! (temperature, `top_p`, `top_k`, `max_tokens`, `repeat_penalty`,
//! `presence_penalty`, `min_p`).
//!
//! **Tier B — Policy** ([ADR 0001]) for the hierarchy: the ordered fold,
//! profiles, the user-set versus auto-detected split and the class floors are
//! decisions llama-server is structurally not in a position to make, so
//! nothing here gates on [`RuntimeCapabilities`].
//!
//! [`with_hardcoded_defaults`](InferenceConfig::with_hardcoded_defaults) is
//! the exception, and it is the same shape as ADR 0001's `truncation` caveat:
//! the *policy* of having a floor is gglib's, but a floor *value* that equals
//! llama.cpp's own default is a redundant assertion rather than a decision.
//! Six of the seven were measured to be exactly that. [ADR 0003] decides they
//! are deferred, leaving `temperature` — the one genuine divergence, 0.7
//! against upstream's 0.8 — plus
//! [`reasoning_floor`](InferenceConfig::reasoning_floor)'s class-aware
//! overrides.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
//! [`RuntimeCapabilities`]: crate::domain::RuntimeCapabilities
//!
//! This module provides the core `InferenceConfig` type that is reused across:
//! - Per-model defaults (`Model.inference_defaults`)
//! - Global settings (`Settings.inference_defaults`)
//! - Request-level overrides (flattened in `ChatProxyRequest`)
//! - `gglib proxy` — per-request injection into OpenAI-format request bodies
//! - `gglib chat` / `gglib q` — hierarchy resolution for the agentic loop
//!
//! All surfaces resolve inference parameters through
//! [`InferenceConfig::resolve_with_profile`], which is the single source of
//! truth for the hierarchy. [`InferenceConfig::resolve_with_defaults`] is the
//! same resolution with no profile selected, for surfaces that have no notion
//! of one.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::reasoning_effort::ReasoningEffort;
use crate::domain::sampling_provenance::{FieldSources, ParamSource};

/// Inference parameters for LLM sampling.
///
/// All fields are optional to support partial configuration and fallback chains.
/// Intended to be shared across model defaults, global settings, and request overrides.
///
/// # Hierarchy Resolution
///
/// When making an inference request, parameters are resolved in this order:
/// 1. Request-level override (user specified for this request)
/// 2. Selected profile (`Settings.inference_profiles`, chosen as
///    `{model}:{profile}`; absent on surfaces without profiles)
/// 3. Per-model defaults (stored in `Model.inference_defaults`)
/// 4. Global settings (stored in `Settings.inference_defaults`)
/// 5. Hardcoded fallback (e.g., temperature = 0.7)
///
/// # Examples
///
/// ```rust
/// use gglib_core::domain::InferenceConfig;
///
/// // Conservative settings for code generation
/// let code_gen = InferenceConfig {
///     temperature: Some(0.2),
///     top_p: Some(0.9),
///     top_k: Some(40),
///     max_tokens: Some(2048),
///     repeat_penalty: Some(1.1),
///     ..Default::default()
/// };
///
/// // Creative writing settings
/// let creative = InferenceConfig {
///     temperature: Some(1.2),
///     top_p: Some(0.95),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InferenceConfig {
    /// Sampling temperature (0.0 - 2.0).
    ///
    /// Controls randomness in token selection:
    /// - Lower values (0.1-0.5): More deterministic, focused
    /// - Medium values (0.7-1.0): Balanced creativity
    /// - Higher values (1.1-2.0): More random, creative
    pub temperature: Option<f32>,

    /// Nucleus sampling threshold (0.0 - 1.0).
    ///
    /// Considers only the top tokens whose cumulative probability exceeds this threshold.
    /// Common values: 0.9 (default), 0.95 (more diverse)
    pub top_p: Option<f32>,

    /// Top-K sampling limit.
    ///
    /// Considers only the K most likely next tokens.
    /// Common values: 40 (default), 10 (focused), 100 (diverse)
    pub top_k: Option<i32>,

    /// Maximum tokens to generate in response.
    ///
    /// Hard limit on response length. Does not include input tokens.
    pub max_tokens: Option<u32>,

    /// Repetition penalty (> 0.0, typically 1.0 - 1.3).
    ///
    /// Penalizes repeated tokens to reduce repetitive output.
    /// - 1.0: No penalty (default)
    /// - 1.1-1.3: Moderate penalty
    /// - > 1.3: Strong penalty (may hurt coherence)
    pub repeat_penalty: Option<f32>,

    /// Presence penalty (0.0 - 2.0).
    ///
    /// Penalizes tokens that have already appeared in the output, encouraging
    /// the model to cover new ground. Effective at preventing repetitive
    /// reasoning loops in thinking models.
    /// - 0.0: No penalty (default; disabled)
    /// - 1.5: Recommended for reasoning/thinking models (e.g. `Qwen3.6`, `DeepSeek-R1`)
    /// - > 2.0: Avoid; may degrade coherence
    pub presence_penalty: Option<f32>,

    /// Frequency penalty (-2.0 - 2.0), `presence_penalty`'s twin.
    ///
    /// Penalizes tokens in proportion to how *often* they have already
    /// appeared, where `presence_penalty` is a flat once-seen offset. An
    /// OpenAI-standard field that llama.cpp supports; until it was modelled
    /// here it passed through the proxy ungoverned, so an untrusted client
    /// could steer sampling with it while every modelled twin was gated
    /// (ADR 0003's `frequency_penalty` follow-up).
    /// - 0.0: No penalty (llama.cpp's default)
    /// - Negative values *encourage* reuse; valid upstream, rarely wanted
    ///
    /// Unset defers to llama.cpp's own default (0.0, disabled).
    ///
    /// Deliberately **not** part of the temperature-coupled trio, although
    /// the trio's rationale (a flat logit offset competing with temperature's
    /// sharpening) applies to it literally. The cost that evicted DRY from
    /// the coupled set (#746) applies literally too: coupling would make a
    /// layer naming only a `frequency_penalty` lose it to any lower layer
    /// naming a `temperature` — the default state of every `reasoning`-tagged
    /// model — and no shipped profile pairs a frequency penalty with a
    /// temperature, so coupling would protect nothing in exchange. The trio
    /// stays a closed set; joining it needs sweep data, not symmetry.
    pub frequency_penalty: Option<f32>,

    /// Minimum-probability sampling threshold (0.0 - 1.0).
    ///
    /// Removes tokens whose probability is below `min_p × P(top token)`.
    /// - 0.0: Disabled (explicit off; recommended by Qwen3.6, and the floor
    ///   for `reasoning`-tagged models — see [`reasoning_floor`])
    /// - 0.05: llama.cpp's own default, and the neutral floor here — see
    ///   [`with_hardcoded_defaults`]
    ///
    /// [`reasoning_floor`]: Self::reasoning_floor
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    pub min_p: Option<f32>,

    /// Dynamic-temperature half-range — entropy-adaptive temperature.
    ///
    /// llama.cpp scales the effective temperature within
    /// `[temperature − range, temperature + range]` by the entropy of each
    /// step's token distribution: confident (low-entropy) steps decode
    /// cooler, uncertain (high-entropy) steps decode hotter. On a completion
    /// that mixes free-form reasoning with structured tool-call tokens this
    /// is a per-token soft version of phase-aware sampling, with no phase
    /// detection required.
    /// - 0.0: Disabled (llama.cpp's default)
    /// - 0.3–0.75: Exploratory range around a 0.6–1.0 base temperature
    ///
    /// Unset defers to llama.cpp's own default (0.0, disabled).
    ///
    /// Deliberately **not** part of the temperature-coupled trio, for the
    /// reason DRY is not (#746): coupling would make a layer naming only a
    /// `dynatemp_range` lose it to any lower layer naming a `temperature` —
    /// the default state of every `reasoning`-tagged model — which costs the
    /// most natural way of switching it on. Unlike the trio, its meaning is
    /// anchored to whatever base temperature actually resolves, so an
    /// orphaned range stays coherent.
    pub dynatemp_range: Option<f32>,

    /// Dynamic-temperature exponent, shaping how sharply the effective
    /// temperature responds to entropy. Inert while
    /// [`dynatemp_range`](Self::dynatemp_range) is unset or zero.
    /// Unset defers to llama.cpp's own default (1.0).
    pub dynatemp_exponent: Option<f32>,

    /// Top-n-sigma: keep only tokens whose *pre-softmax* logit is within
    /// `n × σ` of the maximum (arXiv 2411.07641). Because it truncates the
    /// unscaled logits, the candidate set does not widen as temperature
    /// rises — the property that makes it a candidate for keeping tool-call
    /// tokens stable while a reasoning model decodes hot.
    /// - ≤ 0.0: Disabled (llama.cpp's default is −1.0)
    /// - 1.0–2.0: The paper's evaluated range
    ///
    /// Unset defers to llama.cpp's own default (−1.0, disabled).
    pub top_n_sigma: Option<f32>,

    /// DRY (Don't Repeat Yourself) penalty strength.
    ///
    /// Penalises tokens that would extend a sequence already present in the
    /// context, which catches the multi-token degenerate loops that
    /// `repeat_penalty` — a flat per-token penalty — cannot see.
    /// - 0.0: Disabled (llama.cpp's default, and the floor here)
    /// - 0.8: A common starting point for long agentic sessions
    ///
    /// Left alone on agentic turns, deliberately. An earlier version forced
    /// this to `0` whenever a request carried tools, reasoning that structured
    /// output legitimately repeats tokens. Both halves were wrong: llama.cpp's
    /// sequence breakers already default to `\n`, `:`, `"`, `*` — two of which
    /// are pervasive in JSON — and agentic clients send `tools` on *every*
    /// request, so the pin would have disabled DRY for whole sessions, which
    /// is the workload it exists for.
    ///
    /// llama.cpp's fifth DRY parameter, `--dry-sequence-breaker`, is not
    /// modelled: it is a list of strings, and every layer of this hierarchy —
    /// merge, coupling, the CLI flags, the settings mirror — is built for
    /// scalars. It is also the right lever if DRY is ever seen mangling a tool
    /// call, so modelling it is the follow-up, not switching DRY off.
    pub dry_multiplier: Option<f32>,

    /// DRY penalty base, the exponent applied per token of matched sequence
    /// length. Higher grows the penalty faster on longer repeats.
    /// Unset defers to llama.cpp's own default (1.75).
    pub dry_base: Option<f32>,

    /// Sequence length, in tokens, that DRY tolerates before penalising.
    /// Unset defers to llama.cpp's own default (2).
    pub dry_allowed_length: Option<i32>,

    /// How far back DRY scans for repeats, in tokens. `0` disables the
    /// penalty; llama.cpp resolves negative values against the context size.
    /// Unset defers to llama.cpp's own default (64).
    pub dry_penalty_last_n: Option<i32>,

    /// RNG seed for the sampler.
    ///
    /// Unset means llama.cpp draws a fresh random seed per request, which is
    /// what it reports as `4294967295` (`u32::MAX`) in `/slots`.
    ///
    /// # Request-scoped by design, like `max_tokens`
    ///
    /// This is the second field in this struct that is **not a sampling
    /// policy**. Every other one answers "how should this model sample?" and is
    /// worth storing per model; a seed answers "make *this run* reproducible",
    /// and a seed stored per model would pin every response that model ever
    /// produces to the same text. So:
    ///
    /// - no floor names it, on either class floor;
    /// - nothing gglib writes at import names it;
    /// - the CLI, profile and settings surfaces do not expose it.
    ///
    /// It lives here anyway, rather than beside the hierarchy, for the reason
    /// `max_tokens` does: this struct is the single thing that becomes the
    /// request body ([`Self::to_openai_json_patch`]), and a value that reaches
    /// the wire any other way is a value the ladder cannot explain and the
    /// [readback] cannot check. llama.cpp reports the applied seed in
    /// `/slots`, so routing it through here is what makes "did my seed actually
    /// land?" an answerable question rather than an assumption — which is the
    /// whole point of seeding a benchmark.
    ///
    /// [readback]: https://github.com/mmogr/gglib/blob/main/crates/gglib-proxy/src/sampling_audit.rs
    pub seed: Option<u32>,

    /// How hard the model is *asked* to think — a **prompt-shaping template
    /// control**, not a sampler.
    ///
    /// Every other field in this struct configures llama.cpp's sampler chain.
    /// This one does not touch it. It is parsed off the top-level `OpenAI`
    /// body, stored as a Jinja kwarg and handed to the chat template, which
    /// may render it into the prompt, may branch on it, or — on most models —
    /// may never read the variable at all. It changes what the model is shown,
    /// not how its logits are cut.
    ///
    /// # The wire, measured
    ///
    /// Against the pinned build ([ADR 0007] finding 7c):
    ///
    /// | sent | llama-server | why it matters here |
    /// |---|---|---|
    /// | `"high"` | 200, kwarg set | the ordinary case |
    /// | `"banana"` | 200, **rendered into the prompt verbatim** | nothing upstream validates this field |
    /// | `42` | 200, kwarg dropped, template default applies | a wrong type degrades silently |
    /// | `""` | 200, ignored | already means "no opinion" |
    /// | `"none"` | 200, kwarg **erased** | on `gpt-oss` the template's own `medium` fallback then fires |
    ///
    /// # An enum makes gglib stricter than upstream, deliberately
    ///
    /// [`extract_client_sampling`]'s doctrine is to "accept what upstream
    /// accepts and reject what upstream rejects, so gglib never becomes the
    /// stricter of the two". A closed [`ReasoningEffort`] plainly violates it:
    /// llama-server takes `"banana"` and gglib will not.
    ///
    /// The doctrine's premise is that upstream is the authority on what a
    /// field means, so disagreeing with it can only cost a value that would
    /// have worked. That premise is false for exactly this field. ADR 0007
    /// finding 7c measured the asymmetry: upstream **governs the budget** —
    /// `reasoning_budget_tokens: -2` is a clean HTTP 400 naming the range —
    /// and **does not govern effort at all**. There is no allowlist, no type
    /// check, and no check that the loaded template reads the variable. Where
    /// upstream has no opinion, gglib's governance is not the *stricter* of
    /// two; it is the only one there is.
    ///
    /// And the failure mode is not a rejected-but-valid value. `"banana"` does
    /// not fail, it renders: the user's own prompt gains a line reading
    /// `Reasoning: banana`, and the model answers as if a person had typed it.
    /// Passing that through is not accepting what upstream accepts, it is
    /// forwarding a typo into a prompt. A rejected field costs the client its
    /// `reasoning_effort` and is reported by name
    /// ([`FieldIssue::Rejected`]); a forwarded typo costs the answer and is
    /// reported nowhere.
    ///
    /// That trade only exists if the rejection also *removes the key*, and it
    /// is this field that forces the point. Every other reader here rejects
    /// only what upstream would have rejected too, so a forwarded reject came
    /// back as an HTTP 400 and the damage was visible; and floored fields are
    /// overwritten by the resolved patch anyway. This field is neither — no
    /// layer or floor ever names it (`no_floor_names_a_reasoning_control`) and
    /// upstream validates nothing — so a rejection that stopped at the layer
    /// would be a report with no effect, and `"banana"` would reach the
    /// prompt with gglib's own `client_fields_rejected` recording that it had
    /// been stopped. See the body cleanup in `request_pipeline::sampling`, and
    /// `a_rejected_effort_level_never_reaches_the_wire` for the pin.
    ///
    /// The narrower rule the doctrine actually encodes still holds and is
    /// obeyed by this field's twin: see
    /// [`reasoning_budget_tokens`](Self::reasoning_budget_tokens), which
    /// accepts upstream's full `-1..=i32::MAX` and rejects precisely what
    /// upstream 400s on.
    ///
    /// # Permanently Blind
    ///
    /// Neither this nor the budget appears in `/slots` or `/props`;
    /// `task_params::to_json` exports no reasoning field in either branch. [ADR
    /// 0004]'s readback can confirm a `top_k` arrived and can never confirm
    /// this did. The provenance record is the whole account — which is why the
    /// field is modelled here rather than left to ride the body ungoverned
    /// (ADR 0007 finding 6, the #779 shape under a new name).
    ///
    /// # No floor names one
    ///
    /// See [`with_hardcoded_defaults`]. Several templates default themselves
    /// (`gpt-oss` to `medium`); a floor here would override each template's own
    /// choice with a value nobody made.
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    /// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
    /// [`extract_client_sampling`]: Self::extract_client_sampling
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    pub reasoning_effort: Option<ReasoningEffort>,

    /// How many tokens of thinking the model is allowed before it is cut off —
    /// a **budget**, not a sampler and not a taste.
    ///
    /// It says what this request *is* (a turn that may spend at most `n`
    /// tokens reasoning), the same category `max_tokens` occupies, and it is
    /// enforced by llama.cpp itself (`common/reasoning-budget.{h,cpp}`) rather
    /// than by a template that may or may not read a variable. That is what
    /// puts it on the client-authoritative side of the trust gate while its
    /// twin [`reasoning_effort`](Self::reasoning_effort) is gated — see
    /// [`crate::request_pipeline::sampling`], which states the split where the
    /// carve-out is coded.
    ///
    /// # The wire, measured — and the range is upstream's, not gglib's
    ///
    /// Range-validated on the pinned build: `-1 <= v <= 2147483647`, and `-2`
    /// comes back as a clean HTTP 400 naming the range ([ADR 0007]
    /// finding 7c). So the full `-1..=i32::MAX` is accepted here and only
    /// values below `-1` are rejected — gglib matches upstream's own 400
    /// exactly and adds nothing. This is the doctrine at
    /// [`extract_client_sampling`] applied straight, and it is the reason the
    /// departure argued on `reasoning_effort` is a departure about *that*
    /// field rather than about reasoning controls in general.
    ///
    /// - `-1` — defer to the launch-time `--reasoning-budget` default.
    /// - `0` — valid, and the honest spelling of "stop thinking immediately".
    ///   It is what gglib offers instead of `reasoning_effort: "none"`, which
    ///   yields *medium* thinking on `gpt-oss` (ADR 0007 decision 4).
    ///
    /// A value gglib refuses (below `-1`) is dropped from the layer and
    /// **left in the forwarded body**, unlike its twin: upstream's own 400
    /// names this field and its range, which is a better answer to the client
    /// than a silent substitution and keeps gglib no stricter than upstream.
    /// See [`FieldIssue::Rejected`].
    ///
    /// # The alias
    ///
    /// Upstream also accepts `thinking_budget_tokens`
    /// ([`THINKING_BUDGET_TOKENS_KEY`]) as a second spelling of this
    /// parameter. gglib reads it — a name it did not read was a name the trust
    /// gate could not govern — and emits the canonical one only, erasing the
    /// alias from every forwarded body so the two spellings cannot disagree
    /// about the resolved value. With both sent, the canonical key wins. See
    /// [`read_reasoning_budget_tokens`].
    ///
    /// # Permanently Blind, like its twin
    ///
    /// It is parsed into `params.sampling`, but `task_params::to_json`
    /// serialises no `reasoning_budget_*` field, so nothing echoes it in
    /// `/slots` or `/props` either. ADR 0007's finding 7a is a correction to
    /// that ADR's own earlier claim; do not re-derive an echo from the request
    /// *parse* table.
    ///
    /// # No floor names one
    ///
    /// See [`with_hardcoded_defaults`]. A fleet-wide thinking budget is a
    /// tuning decision with measurement behind it, and `-1` — the "defer"
    /// sentinel — is exactly what omitting the key already means.
    ///
    /// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
    /// [`extract_client_sampling`]: Self::extract_client_sampling
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    pub reasoning_budget_tokens: Option<i32>,
}

/// Whether a model's stored `inference_defaults` were set by the user or
/// written automatically at import time.
///
/// `Model.inference_defaults` is populated two ways: a user explicitly
/// tunes it (`gglib model update --presence-penalty …`, or the `WebUI` edit
/// form), or [`crate::services`]'s import path auto-writes
/// [`InferenceConfig::reasoning_profile`] onto any model carrying the
/// `reasoning` tag — a reasonable guess, not a user decision. Both end up in
/// the same column with nothing distinguishing them, which meant an
/// auto-written guess silently outranked the user's own global settings in
/// the resolution ladder ([`InferenceConfig::resolve_with_profile`]) exactly
/// as if the user had tuned it themselves.
///
/// This type tracks which one actually happened, so resolution can rank
/// [`AutoDetected`](Self::AutoDetected) below global settings while a real
/// [`User`](Self::User) choice keeps outranking them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultsOrigin {
    /// Set explicitly by the user — a CLI flag, a model-update request, or a
    /// `WebUI` edit. Outranks global settings, same as before this type
    /// existed.
    User,
    /// Written automatically at import time from the model's `reasoning`
    /// tag, never reviewed by a person. Ranks below global settings.
    AutoDetected,
    /// Read at import time from the model author's own
    /// `generation_config.json` on `HuggingFace`.
    ///
    /// Ranks **exactly where [`Self::AutoDetected`] does** — below global
    /// settings — and is a distinct variant because of what it is, not where
    /// it sits. Both are written without a person reviewing them, so neither
    /// may outrank a setting somebody chose.
    ///
    /// It never *coexists* with `AutoDetected`: the import prefers this when
    /// it can fetch one, because a published recipe is evidence about this
    /// model where [`InferenceConfig::reasoning_profile`] is a generic guess
    /// keyed off a tag. So "above generic tag guesses" holds by replacement
    /// rather than by rank, and no new ladder rung is needed to express it.
    ///
    /// Kept apart from `User` for the reason that distinction exists at all:
    /// this is not a value anybody in this installation decided, so the
    /// agentic-turn ceiling may still cap it and global settings still win.
    Published,
    /// Written by a tune sweep: the winner of a measured comparison on this
    /// model, this quant, this hardware — not a person's choice, and not a
    /// guess either.
    ///
    /// Ranks **exactly where [`Self::AutoDetected`] does** — below global
    /// settings. The principle that puts it there is the ladder's oldest one:
    /// nothing a person chose may be outranked by anything a person did not,
    /// and an automated apply is not a person. Like [`Self::Published`], it
    /// never coexists with the other below-global origins: applying a winner
    /// overwrites `inference_defaults`, so "above the guesses" holds by
    /// replacement rather than by rank.
    ///
    /// Unlike both of them, the **agentic-turn ceiling never caps it**. The
    /// sweep resolves its candidates against this model's real context
    /// (#748) precisely so the winner transfers to production; a ceiling
    /// capping the stored winner would un-measure it on exactly the turns it
    /// was measured for. The ceiling exists to bound values nobody examined —
    /// a measured temperature is the most examined value in the system.
    Measured,
}

impl std::fmt::Display for DefaultsOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::AutoDetected => write!(f, "auto_detected"),
            Self::Published => write!(f, "published"),
            Self::Measured => write!(f, "measured"),
        }
    }
}

impl std::str::FromStr for DefaultsOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "auto_detected" => Ok(Self::AutoDetected),
            "published" => Ok(Self::Published),
            "measured" => Ok(Self::Measured),
            other => Err(format!(
                "unknown defaults origin '{other}'; expected user, auto_detected, published or measured"
            )),
        }
    }
}

/// Everything about the target model that changes how sampling resolves,
/// independent of any specific request.
///
/// Bundled rather than passed as separate parameters because both
/// [`InferenceConfig::resolve_with_profile`] and
/// [`crate::request_pipeline::sampling::resolve_sampling`] need the same two
/// facts about the same model, and the list has already grown once (see
/// #685) — a named struct reads at call sites instead of two easily
/// transposed booleans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelSamplingContext {
    /// Whether the model carries gglib's `reasoning` capability tag. Selects
    /// the coupled-trio floor — see [`InferenceConfig::reasoning_floor`].
    pub is_reasoning: bool,
    /// Whether the model's `inference_defaults` were user-set or
    /// auto-detected. `None` when the model has no stored
    /// `inference_defaults` at all, in which case it has no effect either
    /// way. See [`DefaultsOrigin`].
    pub defaults_origin: Option<DefaultsOrigin>,
}

impl ModelSamplingContext {
    /// Read both facts off a catalog row.
    ///
    /// Four call sites across three crates built this struct field-by-field
    /// from a [`Model`](crate::domain::Model), each re-deriving `is_reasoning`
    /// from the tag list inline. Two fields is exactly the size at which
    /// hand-construction looks harmless and stops being so: the pair travels
    /// together, both are read by the same fold, and a call site that filled
    /// one and defaulted the other would resolve against the wrong floor or
    /// mis-rank the model's own defaults against global settings — silently,
    /// in both cases.
    #[must_use]
    pub fn for_model(model: &crate::domain::Model) -> Self {
        Self {
            is_reasoning: crate::domain::capability_tags::is_reasoning(&model.tags),
            defaults_origin: model.defaults_origin,
        }
    }
}

/// Convert a camelCase string to `snake_case`.
///
/// Used internally to rename `InferenceConfig`'s serde camelCase output to the
/// `OpenAI` wire format (`topP` → `top_p`, `maxTokens` → `max_tokens`, etc.).
fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if ch.is_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// What reading one client-supplied sampling field did, when it was not
/// simply "read it".
///
/// Carried out of [`InferenceConfig::extract_client_sampling`] so the caller
/// can log or count it. A value gglib declines to use is a fact about a
/// client worth surfacing — before this existed, the entire client sampling
/// layer could vanish over one key with nothing recording that it had.
///
/// # An issue is a report, not by itself an instruction to the body
///
/// Recording that gglib could not use a value says nothing about whether the
/// client's own spelling should still be forwarded, and the answer is *usually
/// yes*: llama-server rejects what these readers reject, so a forwarded bad
/// value earns the client an honest HTTP 400 from the system that owns the
/// field. One field escapes that rule — [`REASONING_EFFORT_KEY`], which
/// upstream does not validate at all — and it is the only one
/// `request_pipeline::sampling` erases on the strength of an issue. See
/// [`Rejected`](Self::Rejected) and that module's `erase_unadopted_client_keys`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldIssue {
    /// Recognised, but not in the form this field takes. A documented
    /// equivalent was substituted and the value is in use.
    ///
    /// The client's own spelling stays in the forwarded body. Where the
    /// substitute is a value (`top_k: 5.0` → `5`) the resolved patch overwrites
    /// it; where it is "no opinion" (`max_tokens: -1`, `seed: -1`) the ladder
    /// emits nothing and the client's sentinel rides through to llama-server,
    /// which reads it as the same absence this type does.
    Normalised {
        /// Wire key, as the client spelled it.
        field: &'static str,
        /// What arrived, rendered for a log line.
        from: String,
        /// What it was taken to mean.
        to: &'static str,
    },
    /// Not readable as this field's type. **This field alone** is dropped;
    /// every other field the client sent is unaffected.
    ///
    /// Dropped from the client's sampling layer always. Whether the client's
    /// own text is also removed from the forwarded body is one field's
    /// exception, not the rule:
    ///
    /// - [`REASONING_EFFORT_KEY`] is **deleted**. Upstream validates it not at
    ///   all, so a refused-but-forwarded `"banana"` is not answered with a 400
    ///   — it is rendered into the user's prompt (ADR 0007 finding 7c). gglib's
    ///   refusal has to bite here because no other system's will.
    /// - Every other field, including [`REASONING_BUDGET_TOKENS_KEY`], is
    ///   **forwarded as sent**. These readers reject what llama-server rejects,
    ///   so the client gets upstream's own 400 naming the field and its range —
    ///   a better answer than a silent substitution, and it keeps gglib exactly
    ///   as strict as upstream rather than stricter.
    Rejected {
        /// Wire key, as the client spelled it.
        field: &'static str,
        /// What arrived, rendered for a log line.
        value: String,
        /// What the field accepts.
        expected: &'static str,
    },
}

impl FieldIssue {
    /// The wire key this issue is about, exactly as it appears in the request
    /// body.
    ///
    /// Every reader passes the literal key it read, including the one field
    /// with two accepted spellings: a budget sent as
    /// [`THINKING_BUDGET_TOKENS_KEY`] is reported under that name, not under
    /// the canonical one the client never used. So this is directly usable as
    /// a `serde_json::Map` key by the body cleanup in
    /// `request_pipeline::sampling`.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        match self {
            Self::Normalised { field, .. } | Self::Rejected { field, .. } => field,
        }
    }
}

impl fmt::Display for FieldIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normalised { field, from, to } => {
                write!(f, "{field}={from} read as {to}")
            }
            Self::Rejected {
                field,
                value,
                expected,
            } => write!(f, "{field}={value} dropped (expected {expected})"),
        }
    }
}

/// Render a JSON value compactly enough for a log line.
///
/// The budget is bytes, and the cut is taken at a character boundary — this
/// renders a value the *client* sent, so it is arbitrary UTF-8 and `&s[..40]`
/// would panic on the first request whose 40th byte fell inside a character.
/// See [`crate::utils::text`].
fn brief(v: &serde_json::Value) -> String {
    crate::utils::text::truncate_with_ellipsis(&v.to_string(), 40).into_owned()
}

/// Narrow a JSON number to the `f32` every sampling field stores.
///
/// Named rather than inline because an `#[allow]` cannot sit on an
/// expression, and the truncation is deliberate: the wire carries `f64` and
/// `InferenceConfig` has always been `f32`.
#[allow(clippy::cast_possible_truncation)]
const fn narrow(n: f64) -> f32 {
    n as f32
}

/// Read one float field. Absent and `null` are both "no opinion".
fn read_f32(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
    issues: &mut Vec<FieldIssue>,
) -> Option<f32> {
    let v = obj.get(key)?;
    if v.is_null() {
        return None;
    }
    v.as_f64().map_or_else(
        || {
            issues.push(FieldIssue::Rejected {
                field: key,
                value: brief(v),
                expected: "a number",
            });
            None
        },
        |n| Some(narrow(n)),
    )
}

/// Read one integer field.
///
/// A float with no fractional part is accepted, because llama.cpp accepts
/// `top_k: 40.0` and several clients emit every number as a float. A float
/// that would lose information is not.
fn read_i32(
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
    issues: &mut Vec<FieldIssue>,
) -> Option<i32> {
    let v = obj.get(key)?;
    if v.is_null() {
        return None;
    }
    if let Some(n) = v.as_i64() {
        return i32::try_from(n).map_or_else(
            |_| {
                issues.push(FieldIssue::Rejected {
                    field: key,
                    value: brief(v),
                    expected: "a 32-bit integer",
                });
                None
            },
            Some,
        );
    }
    if let Some(f) = v.as_f64()
        && f.fract() == 0.0
        && f >= f64::from(i32::MIN)
        && f <= f64::from(i32::MAX)
    {
        #[allow(clippy::cast_possible_truncation)]
        let n = f as i32;
        issues.push(FieldIssue::Normalised {
            field: key,
            from: brief(v),
            to: "an integer",
        });
        return Some(n);
    }
    issues.push(FieldIssue::Rejected {
        field: key,
        value: brief(v),
        expected: "an integer",
    });
    None
}

/// Read `max_tokens`, which is `u32` internally and `-1` on the wire.
///
/// `-1` is llama.cpp's own idiom for "no limit", and omitting the key means
/// exactly that here — see
/// [`with_hardcoded_defaults`](InferenceConfig::with_hardcoded_defaults),
/// which deliberately leaves `max_tokens` unset. So `-1` is not an error to
/// be reported, it is a spelling of a value this type already has. Any other
/// negative is a client bug.
fn read_max_tokens(
    obj: &serde_json::Map<String, serde_json::Value>,
    issues: &mut Vec<FieldIssue>,
) -> Option<u32> {
    let v = obj.get("max_tokens")?;
    if v.is_null() {
        return None;
    }
    // Not `read_i32_raw(v)?` — `?` would return before recording anything,
    // which is the same silent-drop shape this whole function exists to end.
    let Some(n) = read_i32_raw(v) else {
        issues.push(FieldIssue::Rejected {
            field: "max_tokens",
            value: brief(v),
            expected: "a non-negative integer, or -1 for no limit",
        });
        return None;
    };
    if n == -1 {
        issues.push(FieldIssue::Normalised {
            field: "max_tokens",
            from: "-1".to_string(),
            to: "no limit",
        });
        return None;
    }
    u32::try_from(n).map_or_else(
        |_| {
            issues.push(FieldIssue::Rejected {
                field: "max_tokens",
                value: brief(v),
                expected: "a non-negative integer, or -1 for no limit",
            });
            None
        },
        Some,
    )
}

/// Read the `seed` field.
///
/// llama.cpp's own spelling for "pick one at random" is `-1`, and its `/slots`
/// reports a random seed as `4294967295` (`u32::MAX`). Both are normalised to
/// `None`, which is how this type spells the same thing — omission already
/// means random, so carrying a sentinel would give the same state two
/// representations and make `seed.is_some()` stop meaning "reproducible".
fn read_seed(
    obj: &serde_json::Map<String, serde_json::Value>,
    issues: &mut Vec<FieldIssue>,
) -> Option<u32> {
    let v = obj.get("seed")?;
    if v.is_null() {
        return None;
    }
    let Some(n) = v.as_i64() else {
        issues.push(FieldIssue::Rejected {
            field: "seed",
            value: brief(v),
            expected: "a non-negative integer, or -1 for a random seed",
        });
        return None;
    };
    if n == -1 || n == i64::from(u32::MAX) {
        issues.push(FieldIssue::Normalised {
            field: "seed",
            from: brief(v),
            to: "a random seed",
        });
        return None;
    }
    u32::try_from(n).map_or_else(
        |_| {
            issues.push(FieldIssue::Rejected {
                field: "seed",
                value: brief(v),
                expected: "a non-negative integer, or -1 for a random seed",
            });
            None
        },
        Some,
    )
}

/// Wire key for [`InferenceConfig::reasoning_effort`].
///
/// Named rather than spelled twice because the body cleanup in
/// `request_pipeline::sampling` deletes this exact key when the reader refuses
/// a level, and a typo there would be a silent no-op — nothing else in the
/// system would notice a `reasoning_effort` that was never removed.
pub(crate) const REASONING_EFFORT_KEY: &str = "reasoning_effort";

/// Wire key for [`InferenceConfig::reasoning_budget_tokens`], and the only
/// spelling gglib ever *emits*.
pub(crate) const REASONING_BUDGET_TOKENS_KEY: &str = "reasoning_budget_tokens";

/// Upstream's accepted alias for [`REASONING_BUDGET_TOKENS_KEY`].
///
/// llama-server reads either name into the same parameter, so a client may
/// legitimately send this one and mean the budget. gglib therefore *reads* it
/// (see [`read_reasoning_budget_tokens`]) and never emits it: the resolved
/// value is force-inserted under the canonical key alone, and this key is
/// erased from every forwarded body by `request_pipeline::sampling` whatever
/// the trust setting says.
///
/// Both halves are load-bearing. Not reading it left an untrusted client's
/// `thinking_budget_tokens: 100000` riding the body past a gate that governs
/// the canonical spelling — the #779 shape, ungoverned and unrecorded. Not
/// erasing it would leave two keys upstream reads as one, with gglib's own
/// resolved value in only the first: llama-server's own parse order, not
/// gglib's ladder, would decide the budget.
pub(crate) const THINKING_BUDGET_TOKENS_KEY: &str = "thinking_budget_tokens";

/// Read the `reasoning_effort` field.
///
/// The one reader in this module that is stricter than llama-server, and the
/// only one that has to be: upstream validates this field not at all, so an
/// unrecognised level is not caught anywhere downstream — it is *rendered into
/// the prompt*. See [`InferenceConfig::reasoning_effort`] for the argument.
///
/// - a level, in any case → that level
/// - `""` → [`Normalised`](FieldIssue::Normalised) to no opinion; llama-server
///   ignores an empty string, so it already means "unset", and reporting it
///   keeps a client that sends `""` on every request visible.
/// - `"none"` → [`Rejected`](FieldIssue::Rejected) with a pointer at the field
///   that actually stops thinking. Not silently mapped to anything: it is the
///   one wrong value a client is *likely* to send on purpose, and mapping it
///   would guess at an intent (`0` budget? the template default?) that only
///   the client knows.
/// - anything else, including a non-string → `Rejected`.
fn read_reasoning_effort(
    obj: &serde_json::Map<String, serde_json::Value>,
    issues: &mut Vec<FieldIssue>,
) -> Option<ReasoningEffort> {
    const FIELD: &str = REASONING_EFFORT_KEY;

    let v = obj.get(FIELD)?;
    if v.is_null() {
        return None;
    }
    let Some(s) = v.as_str() else {
        issues.push(FieldIssue::Rejected {
            field: FIELD,
            value: brief(v),
            // Deliberately not "a string": a non-string does not fail
            // upstream, it degrades to the template's own default, so naming
            // only the type would leave a client thinking any string works.
            expected: "one of: minimal, low, medium, high, xhigh, max",
        });
        return None;
    };
    if s.is_empty() {
        issues.push(FieldIssue::Normalised {
            field: FIELD,
            from: "\"\"".to_string(),
            to: "no reasoning-effort preference",
        });
        return None;
    }
    if let Some(level) = ReasoningEffort::from_wire(s) {
        return Some(level);
    }
    issues.push(FieldIssue::Rejected {
        field: FIELD,
        value: brief(v),
        expected: if s.eq_ignore_ascii_case("none") {
            // ADR 0007 finding 4: `\"none\"` erases the kwarg, and on gpt-oss
            // the template's own fallback then yields *medium* thinking.
            "one of: minimal, low, medium, high, xhigh, max \
             (\"none\" is not off — use reasoning_budget_tokens: 0)"
        } else {
            "one of: minimal, low, medium, high, xhigh, max"
        },
    });
    None
}

/// Read the `reasoning_budget_tokens` field, under either name upstream
/// accepts for it.
///
/// Exactly upstream's range and nothing narrower: `-1 <= v <= i32::MAX`, with
/// `-1` meaning "defer to the launch `--reasoning-budget`" and `0` meaning
/// "stop thinking immediately". llama-server answers `-2` with an HTTP 400
/// naming that range, so rejecting below `-1` here reproduces upstream's own
/// verdict rather than adding a gglib opinion — the difference from
/// [`read_reasoning_effort`], which has no upstream verdict to reproduce.
///
/// # The alias is read, and the canonical key wins
///
/// llama-server accepts [`THINKING_BUDGET_TOKENS_KEY`] as a second spelling of
/// the same parameter ([ADR 0007] finding 7c). A reader that knew only the
/// canonical name left the alias ungoverned: it entered no layer, appeared in
/// no discard record, was overwritten by no force-insert, and so an untrusted
/// client's `thinking_budget_tokens` outranked the operator's resolved budget
/// silently — the #779 shape this arc exists to close.
///
/// Whichever name arrives, the value becomes
/// [`InferenceConfig::reasoning_budget_tokens`] and is governed like any other
/// client-authoritative budget. With both present the canonical key wins,
/// because that is the name gglib itself emits and the one every other surface
/// (provenance, the audit, `gglib model explain`) reports. An explicit `null`
/// counts as absent under either name, as it does for every other reader here.
///
/// Issues are reported against the key the client actually sent, so a refusal
/// names the text that was in the request rather than a name the client never
/// used.
///
/// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
fn read_reasoning_budget_tokens(
    obj: &serde_json::Map<String, serde_json::Value>,
    issues: &mut Vec<FieldIssue>,
) -> Option<i32> {
    let field = [REASONING_BUDGET_TOKENS_KEY, THINKING_BUDGET_TOKENS_KEY]
        .into_iter()
        .find(|key| obj.get(*key).is_some_and(|v| !v.is_null()))?;

    // `read_i32` has already reported anything unreadable as the key the
    // client sent, so only the range check is left.
    let n = read_i32(obj, field, issues)?;
    if n < -1 {
        issues.push(FieldIssue::Rejected {
            field,
            value: n.to_string(),
            expected: "an integer >= -1 (-1 defers to the launch default, 0 stops thinking)",
        });
        return None;
    }
    Some(n)
}

/// The integer read behind [`read_max_tokens`], without issue reporting —
/// its caller reports in terms of `max_tokens`' own accepted range.
fn read_i32_raw(v: &serde_json::Value) -> Option<i32> {
    if let Some(n) = v.as_i64() {
        return i32::try_from(n).ok();
    }
    let f = v.as_f64()?;
    if f.fract() != 0.0 || f < f64::from(i32::MIN) || f > f64::from(i32::MAX) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(f as i32)
}

/// Which ladder rung supplied each member of the temperature-coupled set.
///
/// Purely an intermediate inside [`InferenceConfig::resolve_layers_with_sources`]:
/// the two arms of the coupling rule each produce one of these, and the
/// provenance record is built from it. Named fields rather than a tuple
/// because both arms and the provenance construction read it positionally
/// otherwise, and the three are easy to transpose.
#[derive(Debug, Clone, Copy)]
struct CoupledLayers {
    repeat_penalty: Option<usize>,
    presence_penalty: Option<usize>,
    min_p: Option<usize>,
}

impl InferenceConfig {
    /// Merge another config into this one, preferring values from `other`.
    ///
    /// For each field, if `other` has Some(value), use it; otherwise keep self's value.
    /// This is useful for applying fallback chains.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::InferenceConfig;
    ///
    /// let mut request = InferenceConfig {
    ///     temperature: Some(0.8),
    ///     ..Default::default()
    /// };
    ///
    /// let model_defaults = InferenceConfig {
    ///     temperature: Some(0.5),
    ///     top_p: Some(0.9),
    ///     ..Default::default()
    /// };
    ///
    /// request.merge_with(&model_defaults);
    /// assert_eq!(request.temperature, Some(0.8)); // Request value wins
    /// assert_eq!(request.top_p, Some(0.9));      // Fallback to model default
    /// ```
    pub const fn merge_with(&mut self, other: &Self) {
        if self.temperature.is_none() {
            self.temperature = other.temperature;
        }
        if self.top_p.is_none() {
            self.top_p = other.top_p;
        }
        if self.top_k.is_none() {
            self.top_k = other.top_k;
        }
        if self.max_tokens.is_none() {
            self.max_tokens = other.max_tokens;
        }
        if self.repeat_penalty.is_none() {
            self.repeat_penalty = other.repeat_penalty;
        }
        if self.presence_penalty.is_none() {
            self.presence_penalty = other.presence_penalty;
        }
        if self.frequency_penalty.is_none() {
            self.frequency_penalty = other.frequency_penalty;
        }
        if self.min_p.is_none() {
            self.min_p = other.min_p;
        }
        if self.dynatemp_range.is_none() {
            self.dynatemp_range = other.dynatemp_range;
        }
        if self.dynatemp_exponent.is_none() {
            self.dynatemp_exponent = other.dynatemp_exponent;
        }
        if self.top_n_sigma.is_none() {
            self.top_n_sigma = other.top_n_sigma;
        }
        if self.dry_multiplier.is_none() {
            self.dry_multiplier = other.dry_multiplier;
        }
        if self.dry_base.is_none() {
            self.dry_base = other.dry_base;
        }
        if self.dry_allowed_length.is_none() {
            self.dry_allowed_length = other.dry_allowed_length;
        }
        if self.dry_penalty_last_n.is_none() {
            self.dry_penalty_last_n = other.dry_penalty_last_n;
        }
        // Still `const`: `ReasoningEffort` is a fieldless `Copy` enum and the
        // budget is an `i32`, so both moves are plain copies. A non-`Copy`
        // field here (a `String` level, say) would have forced the keyword off
        // this function and quietly turned every merge into a clone.
        if self.reasoning_effort.is_none() {
            self.reasoning_effort = other.reasoning_effort;
        }
        if self.reasoning_budget_tokens.is_none() {
            self.reasoning_budget_tokens = other.reasoning_budget_tokens;
        }
    }

    /// Write the temperature-coupled set into `result` and report which rung
    /// supplied each member.
    ///
    /// Split out of [`resolve_layers_with_sources`] only for length; that is
    /// also where the rule it implements is documented. `temperature` is the
    /// rung that claimed the temperature, if any — the whole coupling rule
    /// hangs off whether that is `Some`.
    ///
    /// [`resolve_layers_with_sources`]: Self::resolve_layers_with_sources
    fn resolve_coupled(
        layers: &[Option<&Self>],
        temperature: Option<usize>,
        result: &mut Self,
    ) -> CoupledLayers {
        let first = |declares: &dyn Fn(&Self) -> bool| -> Option<usize> {
            layers.iter().position(|l| l.is_some_and(declares))
        };

        // The layer claiming `temperature` supplies the whole set, including
        // the fields it left unset — those drop to the floor rather than
        // inheriting a value tuned for a temperature nobody chose.
        if let Some(claim) = temperature {
            let c = layers[claim].expect("index came from a Some layer");
            result.repeat_penalty = c.repeat_penalty;
            result.presence_penalty = c.presence_penalty;
            result.min_p = c.min_p;
            return CoupledLayers {
                repeat_penalty: c.repeat_penalty.and(Some(claim)),
                presence_penalty: c.presence_penalty.and(Some(claim)),
                min_p: c.min_p.and(Some(claim)),
            };
        }

        // Nothing was tuned against anything, so the set gap-fills like any
        // uncoupled parameter.
        let found = CoupledLayers {
            repeat_penalty: first(&|c| c.repeat_penalty.is_some()),
            presence_penalty: first(&|c| c.presence_penalty.is_some()),
            min_p: first(&|c| c.min_p.is_some()),
        };
        result.repeat_penalty = found
            .repeat_penalty
            .and_then(|i| layers[i].and_then(|c| c.repeat_penalty));
        result.presence_penalty = found
            .presence_penalty
            .and_then(|i| layers[i].and_then(|c| c.presence_penalty));
        result.min_p = found.min_p.and_then(|i| layers[i].and_then(|c| c.min_p));
        found
    }

    /// Resolve an ordered list of sampling layers (highest priority first)
    /// into a single fully-resolved config, filling anything still unset from
    /// `floor`, and report which layer supplied each field.
    ///
    /// This is the one fold every multi-layer resolution surface goes
    /// through: [`resolve_with_profile`] wraps it for the simple
    /// request/profile/model/global shape, and
    /// [`crate::request_pipeline::sampling`] builds its own **six**-layer
    /// (`cli`, `client`, `profile`, `model`, `global`, `model auto-detected`)
    /// array and calls it directly. There is exactly one place that decides
    /// what "wins" means.
    ///
    /// Values and provenance come from one pass over one ladder and so cannot
    /// disagree — a second function that re-derived the rules would eventually
    /// explain a decision the resolution did not take, which is exactly what
    /// the `describe_provenance` helper this replaced had already started
    /// doing. See [`FieldSources`] for how to read the second half of the
    /// return; callers wanting only the values take `.0`.
    ///
    /// # Uncoupled parameters
    ///
    /// `top_p`, `top_k`, `max_tokens` and the four DRY parameters gap-fill
    /// independently: each takes the first `Some` value found scanning the
    /// layers top to bottom.
    ///
    /// # Coupled parameters
    ///
    /// `presence_penalty`, `repeat_penalty` and `min_p` are only meaningful
    /// relative to how sharp the sampling distribution is, so they travel with
    /// the `temperature` they were chosen for. [`reasoning_profile`] pairs
    /// `temperature 1.0` with `presence_penalty 1.5` deliberately; a sparse
    /// profile that sets `temperature 0.2` and leaves the penalty unset must
    /// not inherit that `1.5` — that would run a recipe no layer ever
    /// intended, a penalty tuned for a broad distribution applied to a
    /// near-greedy one.
    ///
    /// So: `temperature` resolves to the first layer that sets one. If some
    /// layer does, the coupled trio comes *only* from that same layer — never
    /// a layer beneath it — falling to `floor` for anything that layer itself
    /// left unset. If **no** layer sets a temperature at all, nothing has been
    /// tuned against anything, so the trio gap-fills normally, exactly like
    /// the uncoupled parameters.
    ///
    /// # Why DRY is *not* coupled
    ///
    /// It was, briefly, on the symmetry argument that a repetition penalty is
    /// a repetition penalty. Verification showed the symmetry is false and the
    /// cost is real. `presence_penalty` and `repeat_penalty` are flat logit
    /// offsets competing directly with temperature's sharpening; DRY's
    /// strength is governed by its own `dry_base` and `dry_allowed_length`,
    /// and it targets verbatim sequence repetition — a failure mode that is
    /// *worse* at low temperature, not milder.
    ///
    /// Coupling it meant a layer naming a DRY value but no temperature lost
    /// that value silently whenever any lower layer named one, which is the
    /// default state of every `reasoning`-tagged model. Since no shipped
    /// profile and not [`reasoning_profile`] itself pairs a temperature with
    /// DRY values, the coupling protected nothing and cost the most natural
    /// way to switch DRY on. See #745.
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub fn resolve_layers_with_sources(
        layers: &[Option<&Self>],
        floor: &Self,
    ) -> (Self, FieldSources) {
        // Index into `layers` — not into the flattened iterator — so a caller
        // can map it back to the name it gave that rung.
        let first = |declares: &dyn Fn(&Self) -> bool| -> Option<usize> {
            layers.iter().position(|l| l.is_some_and(declares))
        };

        let mut result = Self::default();

        // Uncoupled: each takes the first layer that names it, independently.
        let top_p = first(&|c| c.top_p.is_some());
        let top_k = first(&|c| c.top_k.is_some());
        let max_tokens = first(&|c| c.max_tokens.is_some());
        let temperature = first(&|c| c.temperature.is_some());
        let dynatemp_range = first(&|c| c.dynatemp_range.is_some());
        let dynatemp_exponent = first(&|c| c.dynatemp_exponent.is_some());
        let top_n_sigma = first(&|c| c.top_n_sigma.is_some());
        let frequency_penalty = first(&|c| c.frequency_penalty.is_some());
        let dry_multiplier = first(&|c| c.dry_multiplier.is_some());
        let dry_base = first(&|c| c.dry_base.is_some());
        let dry_allowed_length = first(&|c| c.dry_allowed_length.is_some());
        let dry_penalty_last_n = first(&|c| c.dry_penalty_last_n.is_some());
        let seed = first(&|c| c.seed.is_some());
        let reasoning_effort = first(&|c| c.reasoning_effort.is_some());
        let reasoning_budget_tokens = first(&|c| c.reasoning_budget_tokens.is_some());

        result.top_p = top_p.and_then(|i| layers[i].and_then(|c| c.top_p));
        result.top_k = top_k.and_then(|i| layers[i].and_then(|c| c.top_k));
        result.max_tokens = max_tokens.and_then(|i| layers[i].and_then(|c| c.max_tokens));
        result.temperature = temperature.and_then(|i| layers[i].and_then(|c| c.temperature));
        result.dynatemp_range =
            dynatemp_range.and_then(|i| layers[i].and_then(|c| c.dynatemp_range));
        result.dynatemp_exponent =
            dynatemp_exponent.and_then(|i| layers[i].and_then(|c| c.dynatemp_exponent));
        result.top_n_sigma = top_n_sigma.and_then(|i| layers[i].and_then(|c| c.top_n_sigma));
        result.frequency_penalty =
            frequency_penalty.and_then(|i| layers[i].and_then(|c| c.frequency_penalty));
        result.dry_multiplier =
            dry_multiplier.and_then(|i| layers[i].and_then(|c| c.dry_multiplier));
        result.dry_base = dry_base.and_then(|i| layers[i].and_then(|c| c.dry_base));
        result.dry_allowed_length =
            dry_allowed_length.and_then(|i| layers[i].and_then(|c| c.dry_allowed_length));
        result.dry_penalty_last_n =
            dry_penalty_last_n.and_then(|i| layers[i].and_then(|c| c.dry_penalty_last_n));
        // Uncoupled, and never coupled: a seed says nothing about how sharp the
        // distribution is, so pairing it with a temperature would be
        // meaningless. See the field docs for why no floor names it either.
        result.seed = seed.and_then(|i| layers[i].and_then(|c| c.seed));
        // Uncoupled, and never coupled — for a stronger reason than the seed's.
        // The coupled trio travels with `temperature` because all four shape
        // one probability distribution. Neither reasoning control touches the
        // sampler chain's distribution at all: effort is a template kwarg
        // consumed at render time, and the budget is a token count. Joining
        // them to the trio would mean a profile naming only an effort level
        // stripped a model's tuned `presence_penalty` — a recipe nobody wrote,
        // built out of a field that cannot interact with it.
        result.reasoning_effort =
            reasoning_effort.and_then(|i| layers[i].and_then(|c| c.reasoning_effort));
        result.reasoning_budget_tokens =
            reasoning_budget_tokens.and_then(|i| layers[i].and_then(|c| c.reasoning_budget_tokens));

        let coupled_layers = Self::resolve_coupled(layers, temperature, &mut result);

        result.merge_with(floor);

        // A field no layer claimed came from the floor — or from nowhere, when
        // the floor has none either, which is whatever `with_hardcoded_defaults`
        // leaves unset rather than a list worth restating here.
        //
        // The coupling rule is checked **before** the floor's emptiness, and
        // the order is load-bearing. `Unset` means "nobody named this"; when a
        // layer named it and the coupling rule passed it over, that is a
        // different and more interesting fact, and it stays true whether or
        // not the floor then had a value to offer. Testing `!has_floor` first
        // was harmless while the floor filled all seven, and became a silent
        // loss of provenance the moment ADR 0003 emptied six of them — the
        // coupled trio would have reported as a plain absence.
        let coupled = temperature.is_some();
        let source = |won: Option<usize>, has_floor: bool, is_coupled: bool| match won {
            Some(i) => ParamSource::Layer(i),
            None if is_coupled => ParamSource::FloorCoupled,
            None if !has_floor => ParamSource::Unset,
            None => ParamSource::Floor,
        };

        let sources = FieldSources {
            temperature: source(temperature, floor.temperature.is_some(), false),
            top_p: source(top_p, floor.top_p.is_some(), false),
            top_k: source(top_k, floor.top_k.is_some(), false),
            presence_penalty: source(
                coupled_layers.presence_penalty,
                floor.presence_penalty.is_some(),
                coupled,
            ),
            repeat_penalty: source(
                coupled_layers.repeat_penalty,
                floor.repeat_penalty.is_some(),
                coupled,
            ),
            min_p: source(coupled_layers.min_p, floor.min_p.is_some(), coupled),
            dynatemp_range: source(dynatemp_range, floor.dynatemp_range.is_some(), false),
            dynatemp_exponent: source(dynatemp_exponent, floor.dynatemp_exponent.is_some(), false),
            top_n_sigma: source(top_n_sigma, floor.top_n_sigma.is_some(), false),
            frequency_penalty: source(frequency_penalty, floor.frequency_penalty.is_some(), false),
            dry_multiplier: source(dry_multiplier, floor.dry_multiplier.is_some(), false),
            dry_base: source(dry_base, floor.dry_base.is_some(), false),
            dry_allowed_length: source(
                dry_allowed_length,
                floor.dry_allowed_length.is_some(),
                false,
            ),
            dry_penalty_last_n: source(
                dry_penalty_last_n,
                floor.dry_penalty_last_n.is_some(),
                false,
            ),
            max_tokens: source(max_tokens, floor.max_tokens.is_some(), false),
            reasoning_effort: source(reasoning_effort, floor.reasoning_effort.is_some(), false),
            reasoning_budget_tokens: source(
                reasoning_budget_tokens,
                floor.reasoning_budget_tokens.is_some(),
                false,
            ),
        };

        (result, sources)
    }

    /// The floor beneath every sampling ladder: what gglib asserts when no
    /// layer named a value.
    ///
    /// # It asserts one parameter, not seven
    ///
    /// [ADR 0003] measured this floor against a bare `llama-server` on the
    /// pinned build and found six of its seven values were *exactly* the
    /// upstream default:
    ///
    /// ```text
    ///   parameter          gglib floor   upstream   verdict
    ///   temperature                0.7        0.8   DIVERGES -> policy
    ///   top_p                     0.95       0.95   EQUALS   -> deleted
    ///   top_k                       40         40   EQUALS   -> deleted
    ///   repeat_penalty             1.0        1.0   EQUALS   -> deleted
    ///   presence_penalty           0.0        0.0   EQUALS   -> deleted
    ///   min_p                     0.05       0.05   EQUALS   -> deleted
    ///   dry_multiplier             0.0        0.0   EQUALS   -> deleted
    /// ```
    ///
    /// Restating a value that is already the answer is not a decision, it is
    /// a redundant assertion — and a costly one, because it silently overrides
    /// whatever upstream chooses next. #739 was exactly that failure: a floor
    /// of `min_p: 0.0` disabled the tail cut on every untuned request, and
    /// nothing in the system was positioned to notice. Six such overrides are
    /// now impossible.
    ///
    /// The six are **deferred**, not disabled. Nothing is emitted for them, so
    /// llama.cpp applies its own default — which on this build is the same
    /// number that used to be written here. Provenance reports them as
    /// [`ParamSource::Unset`](crate::domain::ParamSource::Unset), which is
    /// precisely what deferral is: gglib names no value.
    ///
    /// # `temperature: 0.7` stays, and upstream's is 0.8
    ///
    /// The one genuine policy choice in the set, and stated here because an
    /// undocumented divergence is how the other six became invisible in the
    /// first place. gglib decodes slightly more conservatively than
    /// llama.cpp's default for agentic work.
    ///
    /// # The floor is no longer uniform
    ///
    /// [`reasoning_floor`] still asserts `presence_penalty: 1.0` and
    /// `min_p: 0.0` for `reasoning`-tagged models, which are class-aware
    /// policy llama.cpp has no notion of. So after this change `min_p` is
    /// asserted for reasoning models and deferred for every other model.
    /// That is the correct shape and it needs saying out loud, because it is
    /// the first time the floor has differed by model class in what it
    /// *names* rather than only in what it names it as.
    ///
    /// # Deferral is safe only while the build is pinned
    ///
    /// [ADR 0002] pins the llama.cpp build; [ADR 0004]'s `/props` baseline
    /// check reads the default table back and flags any field that moves.
    /// That pairing is what makes deleting a value behaviour-preserving
    /// rather than hopeful.
    ///
    /// [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    /// [`reasoning_floor`]: Self::reasoning_floor
    ///
    /// # `max_tokens` has no fallback
    ///
    /// It is deliberately `None`. Resolution force-writes every `Some` field
    /// into the outgoing request, so a value here would cap *every* request
    /// that did not name its own — silently truncating long answers. Left
    /// unset, no `max_tokens` key is emitted and llama-server applies its own
    /// `n_predict` default of `-1`, generating until a stop token or the
    /// context limit.
    ///
    /// Omitting the key is exactly equivalent to sending `-1` (llama.cpp's
    /// `has_budget()` treats `-1` as limitless) and is the better of the two:
    /// `max_tokens: -1` is invalid under the `OpenAI` schema, which requires a
    /// positive integer, so a strict client or intermediary proxy may reject
    /// it. Omission keeps the forwarded body `OpenAI`-legal.
    ///
    /// Explicit per-request, per-profile, and per-model values are unaffected —
    /// [`reasoning_profile`] still sets its own ceiling.
    ///
    /// # A note on `min_p`, because it moved twice
    ///
    /// #739 changed it from `0.0` to `0.05`, correctly: `0.0` reads like an
    /// absence but was not one — [`to_openai_json_patch`] drops only `None`,
    /// so the floor was *explicitly disabling* the tail cut on every untuned
    /// request. The fix was right, and the mechanism it used was the problem.
    /// #739 restated upstream's value to keep it "visible as `min_p=floor` in
    /// sampling provenance instead of reporting as unset", which bought
    /// visibility at the price of a permanent silent override. Deferral is the
    /// better answer to the same objection: it reports as unset *because it is
    /// unset*, and [ADR 0004]'s readback names llama.cpp's own number instead
    /// of gglib restating it.
    ///
    /// [`reasoning_profile`]: Self::reasoning_profile
    /// [`to_openai_json_patch`]: Self::to_openai_json_patch
    #[must_use]
    pub const fn with_hardcoded_defaults() -> Self {
        Self {
            // No floor names a seed. See the field docs: a seed is not a
            // sampling policy, and a floor that pinned one would make every
            // untuned request in the installation decode identically.
            seed: None,
            // The one value gglib asserts. Upstream's is 0.8; see above.
            temperature: Some(0.7),
            // Everything below is deferred to llama.cpp, which is a decision
            // and not an omission — ADR 0003 finding 1 measured each of them
            // equal to the upstream default on the pinned build. Setting any
            // of these again means overriding whatever upstream chooses next,
            // so do it only with a measurement saying upstream is wrong.
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            presence_penalty: None,
            min_p: None,
            // Never floored: modelled after ADR 0003, under its rule. llama.cpp
            // defaults it to 0.0 (off) and no measurement says otherwise; it is
            // governed here so the untrusted-client gate covers it, not so a
            // floor can assert it.
            frequency_penalty: None,
            // Never floored: introduced after ADR 0003, under its rule — the
            // floor asserts only measured divergences from upstream, and no
            // measurement says llama.cpp's own defaults (range 0.0 / exponent
            // 1.0 / sigma −1.0, all "off") are wrong as a fleet-wide floor.
            // Switching either mechanism on is a per-model or per-profile
            // tuning decision with sweep data behind it.
            dynatemp_range: None,
            dynatemp_exponent: None,
            top_n_sigma: None,
            // DRY stays off, and now says so by silence rather than by
            // asserting the zero llama.cpp already defaults to. Enabling it
            // fleet-wide is a tuning decision for a per-model or per-profile
            // layer with sweep data behind it, not for the floor every untuned
            // model lands on.
            dry_multiplier: None,
            // Never had a floor: with the multiplier off they have no effect,
            // and asserting values would claim a recipe nobody has measured.
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
            // No fallback by design — see above.
            max_tokens: None,
            // Neither reasoning control is floored, and neither ever should
            // be. A floored `reasoning_effort` would override each template's
            // *own* internal default with a value nobody chose — `gpt-oss`
            // sets itself to `medium` when no kwarg arrives, and other
            // templates have other defaults or none. That is the #739 shape
            // (a floor silently displacing the value the thing beneath already
            // had) applied to a control that is not even observable
            // afterwards. A floored budget is the same mistake with a number:
            // `-1` already means "defer to the launch default", which is what
            // emitting no key does for free.
            reasoning_effort: None,
            reasoning_budget_tokens: None,
        }
    }

    /// The coupled-trio floor for models tagged `reasoning`.
    ///
    /// [`resolve_layers_with_sources`] falls back to a floor once it has decided which
    /// layer (if any) claims the coupled set and that layer left a field
    /// unset. [`with_hardcoded_defaults`]'s neutral `presence_penalty: 0.0` is
    /// the right floor for most models, but wrong for a `reasoning`-tagged
    /// one: those degrade under greedy or near-greedy decoding into
    /// repetitive reasoning loops (see [`reasoning_profile`], which pairs
    /// `presence_penalty: 1.5` with `temperature: 1.0` specifically to
    /// prevent this). `1.0` keeps a real guard in place at the floor without
    /// asserting the full recipe tuned for a different temperature.
    ///
    /// `min_p` is pinned to `0.0` for the same class-specific reason: Qwen3.6's
    /// published guidance is to disable min-p on these models, which
    /// [`reasoning_profile`] already encodes.
    ///
    /// # These two are now the only class-specific *assertions*
    ///
    /// The neutral floor used to name `min_p: 0.05` and `presence_penalty:
    /// 0.0`, so this function read as "the same seven values, two of them
    /// different". [ADR 0003] deferred both of those to llama.cpp, so it now
    /// reads as "two values the neutral floor does not name at all".
    ///
    /// The consequence is worth stating because it makes the floor non-uniform
    /// in a way it never was: **`min_p` is asserted for reasoning models and
    /// deferred for everything else.** A reasoning model gets `min_p: 0.0` on
    /// the wire; every other model gets no `min_p` key and llama.cpp's own
    /// 0.05. That asymmetry is deliberate — one is a measured divergence from
    /// upstream, the other is agreement with it — but it will look like a bug
    /// to anyone diffing two requests without this paragraph.
    ///
    /// `presence_penalty: 1.0` is the same shape: asserted here, deferred
    /// elsewhere.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    ///
    /// [`resolve_layers_with_sources`]: Self::resolve_layers_with_sources
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub const fn reasoning_floor() -> Self {
        Self {
            presence_penalty: Some(1.0),
            min_p: Some(0.0),
            ..Self::with_hardcoded_defaults()
        }
    }

    /// The highest temperature an agentic turn should decode at, when the
    /// model's class has one.
    ///
    /// A turn that carries tools may emit structured output. This is the
    /// ceiling that caps its temperature — applied by
    /// [`crate::request_pipeline::sampling`] *after* resolution, and only over
    /// a value nobody deliberately chose. It never raises a temperature.
    ///
    /// # Reasoning models have no ceiling — measured, not argued
    ///
    /// A `reasoning` model does not decode its tool call in isolation: the
    /// `<think>` block and the call are one completion under one sampler
    /// configuration, so a cap imposed for the sake of structured output lands
    /// on the reasoning phase too. This shipped as a `0.6` cap (inside the
    /// Qwen3 / DeepSeek-R1 recommended band), and [ADR 0004]'s addendum named
    /// the evidence that would justify changing it. That experiment ran on
    /// 2026-08-10 (tune runs #12–#32, `Qwen3.5-4B` `Q8_0`, 20 paired runs of
    /// the full agentic suite per arm):
    ///
    /// - Recipe temperature `1.0` uncapped beat the `0.6` cap on the paired
    ///   composite 11W–4L–5T, mean +0.067, Wilcoxon one-sided p = 0.0099,
    ///   bootstrap 95% CI [+0.017, +0.116].
    /// - The cost the cap existed to prevent never materialised: tool-call
    ///   formatting tasks passed 100% at `1.0` versus 98.6% at `0.6`.
    /// - The failure the cap was risking did: loop/stagnation triggers were
    ///   *more* frequent under the cap (29/126 vs 22/117) — cooling a
    ///   thinking model manufactures the repetition its own vendors warn
    ///   about, which the proxy's loop guard then rejects.
    ///
    /// So a reasoning model's resolved temperature stands on agentic turns,
    /// which in the shipped default means its auto-detected recipe's `1.0`.
    ///
    /// # `0.3` for everything else — unmeasured, unchanged
    ///
    /// The non-reasoning cap predates that experiment and no non-reasoning
    /// model has been measured against it. It keeps its old rationale (steady
    /// structured output without being greedy) and its old value until it
    /// earns the same treatment: evidence, not argument.
    ///
    /// # Why a ceiling and not a floor
    ///
    /// The floor this replaced could never fire on the models that most needed
    /// it. A `reasoning`-tagged model carries an auto-detected recipe naming
    /// `temperature: 1.0`, and any layer outranks a floor — so the adjustment
    /// was inert on precisely the models used for agentic coding. A ceiling
    /// gated on provenance fires there and stays out of the way everywhere a
    /// person actually made a choice.
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    #[must_use]
    pub const fn agentic_temperature_ceiling(is_reasoning: bool) -> Option<f32> {
        if is_reasoning { None } else { Some(0.3) }
    }

    /// Return a recommended [`InferenceConfig`] profile for reasoning / thinking models.
    ///
    /// Applied automatically at import time when the `"reasoning"` capability tag is
    /// detected (e.g. Qwen3.6, `DeepSeek-R1`, `QwQ`). Values follow the Qwen3.6 upstream
    /// guidance for **thinking mode — general tasks** and are conservative enough to
    /// work well across all thinking-capable models.
    ///
    /// | Parameter | Value | Rationale |
    /// |-----------|-------|-----------|
    /// | `temperature` | 1.0 | Recommended thinking-mode baseline |
    /// | `top_p` | 0.95 | Broad nucleus; standard for reasoning |
    /// | `top_k` | 20 | Tighter than the 40 fallback; suppresses low-quality tokens |
    /// | `max_tokens` | 8192 | Safe out-of-the-box ceiling; increase for complex tasks |
    /// | `repeat_penalty` | 1.0 | No penalty; `presence_penalty` handles anti-repetition |
    /// | `presence_penalty` | 1.5 | Prevents repetitive reasoning loops |
    /// | `min_p` | 0.0 | Explicitly disabled per Qwen3.6 spec |
    ///
    /// Users can override any parameter with `gglib model update <id> --<flag>` or
    /// the equivalent UI control.
    #[must_use]
    pub const fn reasoning_profile() -> Self {
        Self {
            // Never seeded: this recipe is stored per model, and a stored seed
            // would pin every response that model ever produces.
            seed: None,
            temperature: Some(1.0),
            top_p: Some(0.95),
            top_k: Some(20),
            max_tokens: Some(8192),
            repeat_penalty: Some(1.0),
            presence_penalty: Some(1.5),
            min_p: Some(0.0),
            // Deliberately unset, and this must stay that way: legacy rows are
            // classified as auto-detected by comparing their stored defaults
            // against this recipe verbatim (`resolve_defaults_origin`). Rows
            // written before DRY existed deserialize these as `None`, so any
            // value here would make every one of them compare unequal and
            // silently reclassify as user-set, moving them up a resolution
            // rung. The same holds for every field added since — dynatemp,
            // top-n-sigma and frequency_penalty included.
            dry_multiplier: None,
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
            dynatemp_range: None,
            dynatemp_exponent: None,
            top_n_sigma: None,
            frequency_penalty: None,
            // Deliberately unset for the same legacy-row reason as the block
            // above — and independently, because this recipe is keyed off the
            // `reasoning` *tag*, which says a model thinks, not how hard it
            // should be asked to. Nothing observes whether the level landed,
            // so a guess written here would be an unfalsifiable one.
            reasoning_effort: None,
            reasoning_budget_tokens: None,
        }
    }

    /// Resolve inference parameters using the 4-level hierarchy.
    ///
    /// Equivalent to [`resolve_with_profile`] with no profile selected — see
    /// there for the merge order. This is the entry point for surfaces that
    /// have no notion of a named profile (`gglib serve`, `gglib chat`,
    /// `gglib q`, the Web UI chat API).
    ///
    /// `model_ctx` carries the two facts about the target model that change
    /// how resolution behaves — see [`ModelSamplingContext`],
    /// [`resolve_layers_with_sources`] and [`reasoning_floor`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::{InferenceConfig, ModelSamplingContext};
    ///
    /// let request = InferenceConfig { temperature: Some(0.9), ..Default::default() };
    /// let model   = InferenceConfig { temperature: Some(0.5), top_p: Some(0.8), ..Default::default() };
    ///
    /// let resolved = request.resolve_with_defaults(Some(&model), None, ModelSamplingContext::default());
    /// assert_eq!(resolved.temperature, Some(0.9)); // request wins
    /// assert_eq!(resolved.top_p,       Some(0.8)); // model fills in
    /// assert_eq!(resolved.top_k,       None);      // no layer named it, and
    ///                                              // the floor defers top_k
    ///                                              // to llama.cpp (ADR 0003)
    /// ```
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`resolve_layers_with_sources`]: Self::resolve_layers_with_sources
    /// [`reasoning_floor`]: Self::reasoning_floor
    #[must_use]
    pub fn resolve_with_defaults(
        self,
        model: Option<&Self>,
        global: Option<&Self>,
        model_ctx: ModelSamplingContext,
    ) -> Self {
        self.resolve_with_profile(None, model, global, model_ctx)
    }

    /// Resolve inference parameters using the full 5-level hierarchy.
    ///
    /// Applies fallback layers in order, with each layer filling only `None`
    /// fields from `self` — explicit values are never overwritten:
    ///
    /// 1. `self` — caller-supplied overrides (request params, CLI flags, etc.)
    /// 2. `profile` — the named profile the request selected, if any
    /// 3. `model` — per-model stored defaults, *if user-set*
    /// 4. `global` — global settings defaults
    /// 5. `model` again, *if auto-detected* — see below
    /// 6. the model-class floor — [`reasoning_floor`] when
    ///    `model_ctx.is_reasoning`, otherwise [`with_hardcoded_defaults`]
    ///
    /// This is the single source of truth for inference parameter resolution
    /// across every gglib surface that does not need its own layer set;
    /// [`resolve_with_defaults`] delegates here so there is exactly one merge
    /// order to reason about and to test.
    /// [`crate::request_pipeline::sampling`] needs a sixth rung (the client's
    /// own request, sitting *below* the CLI override rather than between
    /// `self` and `profile`) and calls the underlying [`resolve_layers_with_sources`]
    /// directly for that reason — the merge semantics are identical either
    /// way.
    ///
    /// # Why the profile sits above the model
    ///
    /// Selecting `model:coding` is an explicit act by the caller, so it has to
    /// beat the model's stored defaults or it would appear to do nothing on any
    /// model that has them. Because profiles are *sparse* (see
    /// [`crate::domain::inference_profile`]), outranking the model layer costs
    /// nothing for parameters the profile does not set — those still resolve
    /// from the model, which is what keeps one global profile safe to apply
    /// across differing architectures.
    ///
    /// # Why `model` can rank below `global`
    ///
    /// `model` is only ever a stand-in for `Model.inference_defaults`, which
    /// gets written two different ways (see [`DefaultsOrigin`]): a person
    /// tuning it deliberately, or gglib's own import-time guess for any
    /// model tagged `reasoning`. Those deserve different authority. A
    /// deliberate per-model choice should keep outranking the operator's
    /// global defaults — that is what "per-model" means. A guess nobody
    /// reviewed should not: it silently shadowed the user's own configured
    /// global settings, which is how #685 happened. `model_ctx.defaults_origin`
    /// decides which rung `model` occupies for this call — never both at
    /// once, since only one of rungs 3 and 5 is ever populated for a given
    /// model.
    ///
    /// # Temperature-tuned parameters do not fall through
    ///
    /// See [`resolve_layers_with_sources`] for the full rule. In short: once a layer
    /// declares a `temperature`, lower layers may not contribute
    /// `presence_penalty`, `repeat_penalty` or `min_p` — those resolve from
    /// the claiming layer alone, falling to the class floor if it left them
    /// unset.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::{InferenceConfig, ModelSamplingContext};
    ///
    /// // A sparse profile: sets temperature, says nothing about anything else.
    /// let profile = InferenceConfig { temperature: Some(0.2), ..Default::default() };
    /// // A thinking model's stored defaults: 1.5 is tuned for temperature 1.0.
    /// let model = InferenceConfig {
    ///     temperature: Some(1.0),
    ///     presence_penalty: Some(1.5),
    ///     top_k: Some(20),
    ///     ..Default::default()
    /// };
    /// let model_ctx = ModelSamplingContext { is_reasoning: true, ..Default::default() };
    ///
    /// let resolved = InferenceConfig::default()
    ///     .resolve_with_profile(Some(&profile), Some(&model), None, model_ctx);
    ///
    /// assert_eq!(resolved.temperature,      Some(0.2)); // profile beats model
    /// assert_eq!(resolved.presence_penalty, Some(1.0)); // reasoning floor, NOT the model's 1.5
    /// assert_eq!(resolved.top_k,            Some(20));  // untuned: still fills
    /// ```
    ///
    /// [`resolve_layers_with_sources`]: Self::resolve_layers_with_sources
    /// [`reasoning_floor`]: Self::reasoning_floor
    /// [`with_hardcoded_defaults`]: Self::with_hardcoded_defaults
    /// [`resolve_with_defaults`]: Self::resolve_with_defaults
    #[must_use]
    pub fn resolve_with_profile(
        self,
        profile: Option<&Self>,
        model: Option<&Self>,
        global: Option<&Self>,
        model_ctx: ModelSamplingContext,
    ) -> Self {
        self.resolve_with_profile_explained(profile, model, global, model_ctx)
            .0
    }

    /// [`resolve_with_profile`] plus a record of which rung supplied each
    /// field.
    ///
    /// This is the implementation; [`resolve_with_profile`] delegates here and
    /// discards the provenance, so the ladder — including the user/auto rung
    /// split and the floor selection — is built exactly once.
    ///
    /// Map a [`ParamSource::Layer`] index back to a rung with
    /// [`SamplingLayer::from_index`], which is kept beside this ladder for
    /// that purpose.
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`SamplingLayer::from_index`]: crate::domain::SamplingLayer::from_index
    /// [`ParamSource::Layer`]: crate::domain::ParamSource::Layer
    #[must_use]
    pub fn resolve_with_profile_explained(
        self,
        profile: Option<&Self>,
        model: Option<&Self>,
        global: Option<&Self>,
        model_ctx: ModelSamplingContext,
    ) -> (Self, FieldSources) {
        let floor = if model_ctx.is_reasoning {
            Self::reasoning_floor()
        } else {
            Self::with_hardcoded_defaults()
        };
        // Exhaustive on purpose. The catch-all this replaced sent every
        // not-`AutoDetected` origin to the user-set rung, so adding
        // `Published` would silently have ranked a fetched recipe *above*
        // global settings — the one thing an unreviewed origin must never do.
        let (user_model, auto_model) = match model_ctx.defaults_origin {
            Some(
                DefaultsOrigin::AutoDetected | DefaultsOrigin::Published | DefaultsOrigin::Measured,
            ) => (None, model),
            Some(DefaultsOrigin::User) | None => (model, None),
        };
        Self::resolve_layers_with_sources(
            &[Some(&self), profile, user_model, global, auto_model],
            &floor,
        )
    }

    /// Parse inference parameters from an OpenAI-format JSON body
    /// (`snake_case` keys), plus what the read had to reject or normalise.
    ///
    /// Missing keys, explicit `null`s and keys this type does not model all
    /// yield `None` for that field and leave the rest untouched. This is the
    /// inverse of [`Self::to_openai_json_patch`]; a caller with nothing to
    /// report on the rejections takes `.0`.
    ///
    /// # One bad field must not cost the other ten
    ///
    /// This read used to camel-case the whole body and hand it to
    /// `serde_json::from_value(..).unwrap_or_default()`. Serde parses an
    /// object as a unit, so a single wrongly-typed key failed the whole
    /// deserialise and `unwrap_or_default()` returned an all-`None` config —
    /// silently discarding every sampling value the client sent, with no log
    /// and no test covering the failure path.
    ///
    /// Reading field by field means a bad `max_tokens` costs `max_tokens` and
    /// nothing else.
    ///
    /// # The coercion policy is upstream's, not ours
    ///
    /// [ADR 0003] finding 6 measured what llama.cpp itself accepts on the
    /// pinned build, so this does not have to invent a policy:
    ///
    /// | sent | llama.cpp | here |
    /// |---|---|---|
    /// | `max_tokens: -1` | 200 | [`Normalised`] to `None` — omission already means "no limit" |
    /// | `top_k: 40.0` | 200 | accepted as `40`; a *fractional* float is rejected |
    /// | `temperature: "0.7"` | 400 | [`Rejected`] — a numeric string is a client bug, and quietly parsing it teaches nobody |
    ///
    /// The principle is to accept what upstream accepts and reject what
    /// upstream rejects, so gglib never becomes the stricter of the two on a
    /// value that would have worked. Before this change it was: llama.cpp
    /// takes `max_tokens: -1` and gglib threw away the entire layer over it.
    ///
    /// # One field departs from it, and says so
    ///
    /// [`reasoning_effort`](Self::reasoning_effort) is read against a closed
    /// enum, which llama-server is not: it validates that field not at all and
    /// renders `"banana"` into the prompt. The departure is argued on the
    /// field itself rather than here, because the argument is about that
    /// field's measured wire behaviour and not about coercion in general —
    /// its twin [`reasoning_budget_tokens`](Self::reasoning_budget_tokens)
    /// follows the principle exactly, reproducing upstream's own 400 boundary
    /// and nothing narrower.
    ///
    /// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
    /// [`Normalised`]: FieldIssue::Normalised
    /// [`Rejected`]: FieldIssue::Rejected
    #[must_use]
    pub fn extract_client_sampling(value: &serde_json::Value) -> (Self, Vec<FieldIssue>) {
        let Some(obj) = value.as_object() else {
            return (Self::default(), Vec::new());
        };
        let mut issues = Vec::new();

        let cfg = Self {
            temperature: read_f32(obj, "temperature", &mut issues),
            top_p: read_f32(obj, "top_p", &mut issues),
            top_k: read_i32(obj, "top_k", &mut issues),
            max_tokens: read_max_tokens(obj, &mut issues),
            repeat_penalty: read_f32(obj, "repeat_penalty", &mut issues),
            presence_penalty: read_f32(obj, "presence_penalty", &mut issues),
            frequency_penalty: read_f32(obj, "frequency_penalty", &mut issues),
            min_p: read_f32(obj, "min_p", &mut issues),
            dynatemp_range: read_f32(obj, "dynatemp_range", &mut issues),
            dynatemp_exponent: read_f32(obj, "dynatemp_exponent", &mut issues),
            top_n_sigma: read_f32(obj, "top_n_sigma", &mut issues),
            dry_multiplier: read_f32(obj, "dry_multiplier", &mut issues),
            dry_base: read_f32(obj, "dry_base", &mut issues),
            seed: read_seed(obj, &mut issues),
            dry_allowed_length: read_i32(obj, "dry_allowed_length", &mut issues),
            dry_penalty_last_n: read_i32(obj, "dry_penalty_last_n", &mut issues),
            reasoning_effort: read_reasoning_effort(obj, &mut issues),
            reasoning_budget_tokens: read_reasoning_budget_tokens(obj, &mut issues),
        };

        (cfg, issues)
    }

    /// Serialise as an OpenAI-format JSON patch (`snake_case` keys, `Some` fields only).
    ///
    /// Uses `serde` to produce the camelCase form, then renames each key to
    /// `snake_case` via [`camel_to_snake`]. Only `Some` fields are emitted — `None`
    /// values are filtered out. The returned map can be merged directly into an
    /// OpenAI-compatible request body with `body_obj.insert(k, v)`.
    ///
    /// This is the inverse of [`extract_client_sampling`].
    ///
    /// [`extract_client_sampling`]: Self::extract_client_sampling
    #[must_use]
    pub fn to_openai_json_patch(&self) -> serde_json::Map<String, serde_json::Value> {
        let camel = serde_json::to_value(self).unwrap_or_default();
        camel
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (camel_to_snake(k), v.clone()))
            .collect()
    }
}

#[cfg(test)]
#[path = "inference_tests.rs"]
mod inference_tests;
