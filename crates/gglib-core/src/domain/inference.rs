//! Inference configuration types.
//!
//! Defines shared types for configuring LLM inference parameters
//! (temperature, `top_p`, `top_k`, `max_tokens`, `repeat_penalty`,
//! `presence_penalty`, `min_p`).
//!
//! **Tier B — Policy** ([ADR 0001]) for the hierarchy: the ordered fold,
//! profiles and the split between a person's defaults and a model's recipe
//! are decisions llama-server is structurally not in a position to make, so
//! nothing here gates on [`RuntimeCapabilities`].
//!
//! There is no floor beneath the ladder ([ADR 0022]). A field no rung names is
//! not sent, and llama.cpp's own default applies. [ADR 0003] measured six of
//! the old floor's seven values to restate that default already; ADR 0022
//! deferred the seventh, `temperature: 0.7`, and the `reasoning` class's
//! `presence_penalty: 1.0` and `min_p: 0.0`.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
//! [ADR 0022]: https://github.com/mmogr/gglib/blob/main/docs/adr/0022-the-proxy-does-less.md
//! [`RuntimeCapabilities`]: crate::domain::RuntimeCapabilities
//!
//! This module provides the core `InferenceConfig` type that is reused across:
//! - Per-model defaults (`Model.inference_defaults`)
//! - Global settings (`Settings.inference_defaults`)
//! - Request-level overrides (a run's reasoning controls, a chat title's cap)
//! - `gglib proxy` — per-request injection into OpenAI-format request bodies
//! - `gglib chat` / `gglib q` — the flags a person typed for a turn
//!
//! [`InferenceConfig::resolve_layers_with_sources`] is the single source of
//! truth for the hierarchy. The proxy's requests and every chat's have it
//! folded by [`crate::request_pipeline::sampling`]; `gglib model explain`, a
//! `gglib serve` launch and a benchmark's compare request fold it through
//! [`InferenceConfig::resolve_with_profile`] or one of its variants.

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
/// 3. Per-model defaults a person set (stored in `Model.inference_defaults`)
/// 4. Global settings (stored in `Settings.inference_defaults`)
/// 5. The model's recipe: a published or measured one stored in
///    `Model.inference_defaults`, else the `reasoning` recipe for a
///    `reasoning`-tagged model
/// 6. llama.cpp's own defaults, for anything still unset (gglib sends no
///    value for it)
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    /// OpenAI-standard field that llama.cpp supports, modelled here so the
    /// trust gate governs it like its twins (ADR 0003's `frequency_penalty`
    /// follow-up).
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
    /// - 0.0: Disabled (explicit off; recommended by Qwen3.6, and what
    ///   [`reasoning_profile`] names)
    /// - 0.05: llama.cpp's own default, which applies when no rung names one
    ///
    /// [`reasoning_profile`]: Self::reasoning_profile
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
    /// - 0.0: Disabled (llama.cpp's default)
    /// - 0.8: A common starting point for long agentic sessions
    ///
    /// Left alone on agentic turns, deliberately, although structured output
    /// legitimately repeats tokens: llama.cpp's sequence breakers already
    /// default to `\n`, `:`, `"`, `*` — two of which are pervasive in JSON —
    /// and agentic clients send `tools` on *every* request, so pinning this to
    /// `0` there would disable DRY for whole sessions, which is the workload
    /// it exists for.
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
    /// - no recipe gglib resolves names it;
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
    /// back as an HTTP 400 and the damage was visible. This field is not —
    /// no recipe gglib resolves names it (`no_recipe_names_a_reasoning_control`)
    /// and upstream validates nothing — so a rejection that stopped at the layer
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
    /// # No recipe names one
    ///
    /// Several templates default themselves (`gpt-oss` to `medium`); a value
    /// gglib supplied on its own would override each template's own choice
    /// with a value nobody made.
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    /// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
    /// [`extract_client_sampling`]: Self::extract_client_sampling
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
    /// # No recipe names one
    ///
    /// A fleet-wide thinking budget is a tuning decision with measurement
    /// behind it, and `-1` — the "defer" sentinel — is exactly what omitting
    /// the key already means.
    ///
    /// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
    /// [`extract_client_sampling`]: Self::extract_client_sampling
    pub reasoning_budget_tokens: Option<i32>,
}

/// Who wrote a model's stored `inference_defaults`.
///
/// `Model.inference_defaults` is written three ways: a person tunes it
/// (`gglib model update --presence-penalty …`, or the `WebUI` edit form), the
/// import fetches the model author's published recipe, or a tune sweep
/// applies its winner. Only the first is a person's choice, so only the first
/// outranks global settings in the resolution ladder
/// ([`InferenceConfig::resolve_with_profile`]); the other two rank below them.
///
/// The `reasoning` recipe ([`InferenceConfig::reasoning_profile`]) is not
/// stored and has no origin here. Resolution supplies it below global
/// settings for any `reasoning`-tagged model whose row holds no published or
/// measured recipe ([`ModelSamplingContext::model_rungs`]), so a model whose
/// own defaults are cleared resolves exactly as it did on import (ADR 0022).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum DefaultsOrigin {
    /// Set explicitly by the user — a CLI flag, a model-update request, or a
    /// `WebUI` edit. Outranks global settings.
    User,
    /// Read at import time from the model author's own
    /// `generation_config.json` on `HuggingFace`.
    ///
    /// Ranks below global settings, where the `reasoning` recipe does,
    /// because nobody in this installation reviewed it; and it replaces that
    /// recipe rather than merging with it, because a published recipe is
    /// evidence about this model where the `reasoning` recipe is a generic
    /// guess keyed off a tag.
    ///
    /// Kept apart from `User` for the reason that distinction exists at all:
    /// this is not a value anybody in this installation decided, so the
    /// agentic-turn ceiling may still cap it and global settings still win.
    Published,
    /// Written by a tune sweep: the winner of a measured comparison on this
    /// model, this quant, this hardware — not a person's choice, and not a
    /// guess either.
    ///
    /// Ranks below global settings, where [`Self::Published`] does. The
    /// principle that puts it there is the ladder's oldest one: nothing a
    /// person chose may be outranked by anything a person did not, and an
    /// automated apply is not a person. Applying a winner overwrites
    /// `inference_defaults`, so it replaces a published recipe and the
    /// `reasoning` recipe alike.
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
            "published" => Ok(Self::Published),
            "measured" => Ok(Self::Measured),
            other => Err(format!(
                "unknown defaults origin '{other}'; expected user, published or measured"
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
/// facts about the same model — a named struct reads at call sites instead
/// of two easily transposed booleans.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelSamplingContext {
    /// Whether the model carries gglib's `reasoning` capability tag. Supplies
    /// [`InferenceConfig::reasoning_profile`] below global settings when the
    /// row stores no recipe of its own — see [`Self::model_rungs`].
    pub is_reasoning: bool,
    /// Who wrote the model's `inference_defaults`. `None` when the model has
    /// no stored `inference_defaults` at all. See [`DefaultsOrigin`].
    pub defaults_origin: Option<DefaultsOrigin>,
}

/// The recipe a `reasoning`-tagged model resolves with when its row stores
/// none of its own.
static REASONING_RECIPE: InferenceConfig = InferenceConfig::reasoning_profile();

/// Where a model's own sampling sits on the ladder: at most one config above
/// global settings and at most one below.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ModelRungs<'a> {
    /// What a person set for this model. Outranks global settings.
    pub user_set: Option<&'a InferenceConfig>,
    /// A published or measured recipe stored on the row, or else the
    /// `reasoning` recipe for a `reasoning`-tagged model. Ranks below global
    /// settings.
    pub below_global: Option<&'a InferenceConfig>,
}

impl ModelSamplingContext {
    /// Read both facts off a catalog row.
    ///
    /// Four call sites across three crates built this struct field-by-field
    /// from a [`Model`](crate::domain::Model), each re-deriving `is_reasoning`
    /// from the tag list inline. Two fields is exactly the size at which
    /// hand-construction looks harmless and stops being so: the pair travels
    /// together, both are read by the same fold, and a call site that filled
    /// one and defaulted the other would drop the model's recipe or mis-rank
    /// the model's own defaults against global settings — silently, in both
    /// cases.
    #[must_use]
    pub fn for_model(model: &crate::domain::Model) -> Self {
        Self {
            is_reasoning: crate::domain::capability_tags::is_reasoning(&model.tags),
            defaults_origin: model.defaults_origin,
        }
    }

    /// Place a model's stored defaults on the ladder.
    ///
    /// The one decision both ladders read, so `gglib model explain` and the
    /// request pipeline cannot rank a model's defaults differently:
    ///
    /// - stored by a person ([`DefaultsOrigin::User`], or no origin): above
    ///   global settings;
    /// - stored by the import or a tune sweep ([`DefaultsOrigin::Published`],
    ///   [`DefaultsOrigin::Measured`]): below global settings;
    /// - and below global settings, when nothing is stored there, the
    ///   `reasoning` recipe for a `reasoning`-tagged model.
    ///
    /// The last line is why clearing a model's own defaults gives back the
    /// recipe it was imported with: the recipe is never stored, so no edit can
    /// remove it (#1428).
    ///
    /// Exhaustive on purpose: a catch-all would rank any future origin nobody
    /// reviewed above global settings, which is precisely backwards.
    #[must_use]
    pub fn model_rungs(self, stored: Option<&InferenceConfig>) -> ModelRungs<'_> {
        let (user_set, stored_below) = match self.defaults_origin {
            Some(DefaultsOrigin::Published | DefaultsOrigin::Measured) => (None, stored),
            Some(DefaultsOrigin::User) | None => (stored, None),
        };
        ModelRungs {
            user_set,
            below_global: stored_below.or_else(|| self.is_reasoning.then_some(&REASONING_RECIPE)),
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
/// client worth surfacing.
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
/// exactly that here: a `max_tokens` no rung names is not sent. So `-1` is
/// not an error to be reported, it is a spelling of a value this type already
/// has. Any other negative is a client bug.
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
/// Both halves are load-bearing. Not reading it would leave an untrusted
/// client's `thinking_budget_tokens: 100000` riding the body past a gate that
/// governs the canonical spelling — the #779 shape, ungoverned and
/// unrecorded. Not erasing it would leave two keys upstream reads as one,
/// with gglib's own resolved value in only the first: llama-server's own
/// parse order, not gglib's ladder, would decide the budget.
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
/// canonical name would leave the alias ungoverned: in no layer, in no
/// discard record, overwritten by no force-insert, so an untrusted client's
/// `thinking_budget_tokens` would silently outrank the operator's resolved
/// budget — the #779 shape.
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
    if n < crate::settings::REASONING_BUDGET_TOKENS_MIN {
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
/// the coupling rule produces one of these, and the provenance record is built
/// from it. Named fields rather than a tuple because the provenance
/// construction would read it positionally otherwise, and the three are easy
/// to transpose.
#[derive(Debug, Clone, Copy)]
struct CoupledLayers {
    repeat_penalty: Option<usize>,
    presence_penalty: Option<usize>,
    min_p: Option<usize>,
}

impl InferenceConfig {
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
        // With a claim, only the claimant and the layers above it may supply
        // the set: a lower layer's value was tuned for a temperature that
        // lost, and a field the claimant left unset is not sent rather than
        // inherited. Without one, nothing was tuned against anything, so the
        // set gap-fills like any uncoupled parameter.
        let eligible = temperature.map_or(layers.len(), |claim| claim + 1);
        let first = |declares: &dyn Fn(&Self) -> bool| -> Option<usize> {
            layers[..eligible]
                .iter()
                .position(|l| l.is_some_and(declares))
        };

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
    /// into a single config, and report which layer supplied each field.
    ///
    /// A field no layer names stays `None`: it is not sent, and llama.cpp's
    /// own default applies. There is no floor beneath the ladder (ADR 0022).
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
    /// explain a decision the resolution did not take. See [`FieldSources`]
    /// for how to read the second half of the return; callers wanting only
    /// the values take `.0`.
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
    /// layer does, each member of the coupled trio comes from that layer or a
    /// layer above it — never a layer beneath it — and is not sent when none
    /// of those names it. A layer above the claim that names a member without
    /// a temperature is a choice that stands on its own: a person who sets
    /// only `presence_penalty` on a model keeps it, whatever temperature the
    /// model's recipe resolves beneath (ADR 0022). If **no** layer sets a
    /// temperature at all, nothing has been tuned against anything, so the
    /// trio gap-fills normally, exactly like the uncoupled parameters.
    ///
    /// # Why DRY is *not* coupled
    ///
    /// The symmetry argument — a repetition penalty is a repetition penalty —
    /// is false, and coupling has a real cost. `presence_penalty` and
    /// `repeat_penalty` are flat logit offsets competing directly with
    /// temperature's sharpening; DRY's strength is governed by its own
    /// `dry_base` and `dry_allowed_length`, and it targets verbatim sequence
    /// repetition — a failure mode that is *worse* at low temperature, not
    /// milder.
    ///
    /// Coupled, a layer naming a DRY value but no temperature would lose that
    /// value silently whenever any lower layer named one, which is the
    /// default state of every `reasoning`-tagged model. No shipped profile,
    /// [`reasoning_profile`] included, pairs a temperature with DRY values, so
    /// coupling would protect nothing and cost the most natural way to switch
    /// DRY on. See #745.
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`reasoning_profile`]: Self::reasoning_profile
    #[must_use]
    pub fn resolve_layers_with_sources(layers: &[Option<&Self>]) -> (Self, FieldSources) {
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
        // meaningless. See the field docs for why no recipe names it either.
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

        // The coupling rule is checked before plain absence. `Unset` means
        // "nobody eligible named this"; when a temperature was claimed, a
        // layer beneath the claim may well have named a coupled field and been
        // passed over, which is a different and more interesting fact.
        let coupled = temperature.is_some();
        let source = |won: Option<usize>, is_coupled: bool| match won {
            Some(i) => ParamSource::Layer(i),
            None if is_coupled => ParamSource::Coupled,
            None => ParamSource::Unset,
        };

        let sources = FieldSources {
            temperature: source(temperature, false),
            top_p: source(top_p, false),
            top_k: source(top_k, false),
            presence_penalty: source(coupled_layers.presence_penalty, coupled),
            repeat_penalty: source(coupled_layers.repeat_penalty, coupled),
            min_p: source(coupled_layers.min_p, coupled),
            dynatemp_range: source(dynatemp_range, false),
            dynatemp_exponent: source(dynatemp_exponent, false),
            top_n_sigma: source(top_n_sigma, false),
            frequency_penalty: source(frequency_penalty, false),
            dry_multiplier: source(dry_multiplier, false),
            dry_base: source(dry_base, false),
            dry_allowed_length: source(dry_allowed_length, false),
            dry_penalty_last_n: source(dry_penalty_last_n, false),
            max_tokens: source(max_tokens, false),
            reasoning_effort: source(reasoning_effort, false),
            reasoning_budget_tokens: source(reasoning_budget_tokens, false),
        };

        (result, sources)
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
    /// on the reasoning phase too. Measured on `Qwen3.5-4B` (2026-08-10, tune
    /// runs #12–#32; [ADR 0004]'s postscript), the recipe's uncapped `1.0` beat
    /// a `0.6` cap on the paired composite at no cost in tool-call formatting,
    /// and the cap drew *more* loop/stagnation triggers: cooling a thinking
    /// model manufactures the repetition its own vendors warn about.
    ///
    /// So a reasoning model's resolved temperature stands on agentic turns,
    /// which in the shipped default means the `reasoning` recipe's `1.0`.
    ///
    /// # `0.3` for everything else — unmeasured
    ///
    /// No non-reasoning model has been measured against this cap. Its
    /// rationale is steady structured output without being greedy, and it
    /// stays until it earns the same treatment: evidence, not argument.
    ///
    /// # It only lowers a value a rung supplied
    ///
    /// A ceiling gated on provenance caps a recipe nobody here reviewed and
    /// stays out of the way everywhere a person actually made a choice. Where
    /// no rung names a temperature, nothing is sent and the ceiling does not
    /// add one: llama.cpp's own default applies (ADR 0022).
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    #[must_use]
    pub const fn agentic_temperature_ceiling(is_reasoning: bool) -> Option<f32> {
        if is_reasoning { None } else { Some(0.3) }
    }

    /// Return a recommended [`InferenceConfig`] profile for reasoning / thinking models.
    ///
    /// Resolved below global settings for any model carrying the `"reasoning"`
    /// capability tag (e.g. Qwen3.6, `DeepSeek-R1`, `QwQ`) whose row stores no
    /// published or measured recipe — see [`ModelSamplingContext::model_rungs`].
    /// Never stored, so clearing a model's own defaults gives it back. Values
    /// follow the Qwen3.6 upstream guidance for **thinking mode — general
    /// tasks** and are conservative enough to work well across all
    /// thinking-capable models.
    ///
    /// | Parameter | Value | Rationale |
    /// |-----------|-------|-----------|
    /// | `temperature` | 1.0 | Recommended thinking-mode baseline |
    /// | `top_p` | 0.95 | Broad nucleus; standard for reasoning |
    /// | `top_k` | 20 | Tighter than llama.cpp's 40; suppresses low-quality tokens |
    /// | `max_tokens` | 32768 | Room for a long plan; a turn that hits the cap ends mid-thought |
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
            max_tokens: Some(32_768),
            repeat_penalty: Some(1.0),
            presence_penalty: Some(1.5),
            min_p: Some(0.0),
            // Deliberately unset: the Qwen3.6 guidance this recipe follows
            // names none of them, and a value here would apply to every
            // `reasoning`-tagged model with no measurement behind it.
            dry_multiplier: None,
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
            dynatemp_range: None,
            dynatemp_exponent: None,
            top_n_sigma: None,
            frequency_penalty: None,
            // Deliberately unset, because this recipe is keyed off the
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
    /// there for the merge order. This is the entry point for a caller that
    /// has no notion of a named profile: the benchmark's compare request.
    ///
    /// `model_ctx` carries the two facts about the target model that change
    /// how resolution behaves — see [`ModelSamplingContext`] and
    /// [`resolve_layers_with_sources`].
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
    /// assert_eq!(resolved.top_k,       None);      // no layer named it, so
    ///                                              // llama.cpp's own applies
    /// ```
    ///
    /// [`resolve_with_profile`]: Self::resolve_with_profile
    /// [`resolve_layers_with_sources`]: Self::resolve_layers_with_sources
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
    /// 5. the model's recipe — a published or measured one stored on the
    ///    row, or the `reasoning` recipe — see below
    ///
    /// A field none of them names is not sent.
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
    /// # Why a model's recipe ranks below `global`
    ///
    /// `model` is only ever a stand-in for `Model.inference_defaults`, which
    /// a person writes deliberately or the import and a tune sweep write with
    /// nobody reviewing it (see [`DefaultsOrigin`]), and the `reasoning`
    /// recipe is gglib's own guess for any model tagged `reasoning`. Those
    /// deserve different authority. A deliberate per-model choice should keep
    /// outranking the operator's global defaults — that is what "per-model"
    /// means. A recipe nobody reviewed should not: above global, it silently
    /// shadows the user's own configured global settings (#685).
    /// [`ModelSamplingContext::model_rungs`] decides which rung `model`
    /// occupies, and supplies the `reasoning` recipe beneath global settings
    /// whenever nothing published or measured is stored there.
    ///
    /// # Temperature-tuned parameters do not fall through
    ///
    /// See [`resolve_layers_with_sources`] for the full rule. In short: once a layer
    /// declares a `temperature`, lower layers may not contribute
    /// `presence_penalty`, `repeat_penalty` or `min_p` — those resolve from
    /// the claiming layer or one above it, and are not sent when none of those
    /// names them.
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
    /// assert_eq!(resolved.presence_penalty, None);      // NOT the model's 1.5
    /// assert_eq!(resolved.top_k,            Some(20));  // untuned: still fills
    /// ```
    ///
    /// [`resolve_layers_with_sources`]: Self::resolve_layers_with_sources
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
    /// discards the provenance, so the ladder — including the split between a
    /// person's defaults and the model's recipe — is built exactly once.
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
        let rungs = model_ctx.model_rungs(model);
        Self::resolve_layers_with_sources(&[
            Some(&self),
            profile,
            rungs.user_set,
            global,
            rungs.below_global,
        ])
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
    /// Serde parses an object as a unit, so deserialising the whole body fails
    /// on a single wrongly-typed key and, behind `unwrap_or_default()`,
    /// silently discards every sampling value the client sent. Reading field
    /// by field means a bad `max_tokens` costs `max_tokens` and nothing else.
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
    /// value that would have worked.
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
