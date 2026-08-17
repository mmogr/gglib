//! Stage 4–5: resolving what the model is asked to sample with.
//!
//! Unlike [`super::messages`], nothing here reads `messages` — these transforms
//! only ever touch top-level keys.
//!
//! **Tier B — Policy** ([ADR 0001]). llama-server is one process serving one
//! model with no catalog, no profiles and no view of the client, so it cannot
//! arbitrate between a `:coding` profile and a per-model default. The ladder,
//! the trust gate and the provenance are permanently gglib's, and nothing here
//! gates on [`RuntimeCapabilities`].
//!
//! The **floor beneath** the ladder is a separate question with a different
//! answer: six of its seven values were measured to restate llama.cpp's own
//! defaults, which makes them compensation rather than policy. See
//! [ADR 0003], which decides they are deferred.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
//! [`RuntimeCapabilities`]: crate::domain::RuntimeCapabilities

use serde_json::Value;
use tracing::debug;

use super::ModelContext;
use crate::domain::{DefaultsOrigin, FieldIssue, FieldSources, InferenceConfig, ParamSource};

/// The sampling layers that sit *below* the client's own request parameters.
///
/// Grouped because they are only ever used together, at the single point where
/// [`resolve_sampling`] folds them through [`InferenceConfig::resolve_layers_with_sources`].
///
/// The per-model layer is deliberately absent: it arrives with the rest of the
/// per-model facts, as
/// [`ModelContext::inference_defaults`](super::ModelContext::inference_defaults),
/// so no caller has to look the model up twice. The client's own parameters are
/// absent for a different reason — they are read back out of the request body
/// itself, which is what lets one function serve a proxy forwarding an
/// arbitrary client payload and an adapter that built the body from a typed
/// config.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SamplingLayers {
    /// Operator-supplied overrides from the process's own command line
    /// (`gglib proxy --temperature …`), applied *above* the client's request
    /// parameters.
    ///
    /// Above the client deliberately: this is the person running the server
    /// stating what the server does, which cannot be true if any client can
    /// silently outrank it. These previously merged into [`Self::global`],
    /// which sits below the per-model layer — so on any model with stored
    /// `inference_defaults` the flags did nothing at all.
    pub cli_override: Option<InferenceConfig>,
    /// The profile the request selected via `{model}:{profile}`, if any.
    /// Sparse — see [`crate::domain::inference_profile`].
    pub profile: Option<InferenceConfig>,
    /// Global defaults from settings.
    pub global: Option<InferenceConfig>,
    /// Whether the client's own sampling parameters are honoured at all.
    /// From `Settings.trust_client_sampling`. `false` (the default) drops
    /// everything the client sent except `max_tokens` — see the field doc on
    /// `Settings` for why. This is read from the same settings snapshot as
    /// [`Self::global`], which is why it lives here rather than as a
    /// separate parameter threaded through every caller.
    pub trust_client_sampling: bool,
    /// Whether a request carrying tools gets the agentic-turn temperature
    /// ceiling — see [`InferenceConfig::agentic_temperature_ceiling`].
    ///
    /// Set by the caller rather than defaulted on, because the two callers
    /// decide it differently: the proxy reads `Settings.agentic_sampling`
    /// (opt-out — absent means on), while the in-process agent path has no
    /// settings snapshot and enables it unconditionally. `Default` leaves it
    /// off so a bare `SamplingLayers::default()` applies no adjustment.
    pub agentic_adjustments: bool,
}

/// Which class floor sat beneath the ladder.
///
/// `sources` records that a value came from "the floor" but not *which* one,
/// and the explain surfaces cannot show it at all — they resolve stored
/// configuration with no request in hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorClass {
    /// [`InferenceConfig::with_hardcoded_defaults`].
    Default,
    /// [`InferenceConfig::reasoning_floor`] — a `reasoning`-tagged model.
    Reasoning,
}

impl FloorClass {
    /// The label the debug line and the readback use.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Reasoning => "reasoning",
        }
    }
}

/// Everything [`resolve_sampling`] decided, and why.
///
/// # Why this is returned rather than logged
///
/// It used to be neither: `resolve_sampling` computed `sources` and consumed
/// them only inside a `debug!`. Three consequences, all of which cost real
/// defects:
///
/// - **No test could assert on the pipeline's own provenance.** The tests
///   that look like they do build a ladder by hand and call
///   `resolve_layers_with_sources` directly, bypassing this function
///   entirely — and did so five rungs wide against a six-rung ladder.
/// - **The agentic ceiling's provenance interaction is invisible.** Six tests
///   assert the resulting temperature; none could assert what provenance
///   reports when the ceiling bites, because nothing was reachable.
/// - **There is no intent side to compare a readback against.** Verifying
///   that what gglib resolved is what llama-server applied needs both halves,
///   and this is the half that did not exist outside a log line.
#[derive(Debug, Clone, PartialEq)]
pub struct SamplingDecision {
    /// The values written into the body.
    pub resolved: InferenceConfig,
    /// Which rung supplied each one. Indices are into [`Self::layer_names`].
    pub sources: FieldSources,
    /// The ladder's rung names, highest priority first.
    pub layer_names: [&'static str; LADDER_RUNGS],
    /// Which class floor sat beneath it.
    pub floor: FloorClass,
    /// Whether the request was eligible for the agentic-turn ceiling.
    pub agentic_turn: bool,
    /// The ceiling value, if it actually capped the temperature.
    ///
    /// `Some` does **not** mean `sources.temperature` is wrong: the cap is
    /// applied after the fold and deliberately leaves the rung that supplied
    /// the value named, because that rung did supply it — the ceiling capped
    /// what it supplied.
    pub agentic_ceiling_applied: Option<f32>,
    /// Client fields that could not be read as sent. See [`FieldIssue`].
    pub client_fields_rejected: Vec<FieldIssue>,
    /// Client fields dropped by the trust gate rather than by a parse
    /// failure — empty whenever `trust_client_sampling` is on.
    ///
    /// Carries both kinds of drop: modelled fields the gate binned, and
    /// [`UNMODELLED_SAMPLER_KEYS`] stripped from the body itself, which have
    /// no layer to be binned from.
    pub client_fields_discarded: Vec<String>,
    /// Whether the resolved values actually reached `body`.
    ///
    /// `false` when the body was not a JSON object, in which case everything
    /// above describes a resolution that was computed and then not applied.
    /// Distinguishing the two matters to a readback: nothing was sent, so
    /// nothing can diverge.
    pub applied: bool,
}

/// Rungs in the pipeline's ladder: `cli`, `client`, `profile`, `model`,
/// `global`, `model (auto-detected)`.
///
/// Named because three separate doc comments drifted to three different
/// numbers while the ladder stayed six wide, and because the provenance test
/// helper was built five wide against it and so never checked the mapping.
pub const LADDER_RUNGS: usize = 6;

/// Environment kill switch for the agentic-turn adjustments.
///
/// Truthy values (case-insensitive `1`, `true`, `yes`, `on`) disable it for
/// every caller, whatever their settings say — the same contract as
/// [`DISABLE_GRAMMAR_ENV`].
///
/// [`DISABLE_GRAMMAR_ENV`]: super::constrain::DISABLE_GRAMMAR_ENV
pub const DISABLE_AGENTIC_SAMPLING_ENV: &str = "GGLIB_DISABLE_AGENTIC_SAMPLING";

/// Whether [`DISABLE_AGENTIC_SAMPLING_ENV`] is set to a truthy value.
fn agentic_sampling_disabled_via_env() -> bool {
    crate::debug_switches::enabled(DISABLE_AGENTIC_SAMPLING_ENV)
}

/// Resolve the sampling hierarchy into `body`, then pin `cache_prompt`.
///
/// # Force-insert, not `or_insert`
///
/// The client's own parameters are extracted from `body` first, folded
/// through [`InferenceConfig::resolve_layers_with_sources`] alongside cli / profile /
/// model / global, and the fully-resolved result is then written back over
/// the top. Client parameters still win — they win by being the
/// highest-priority *layer* in the fold, not by surviving an `or_insert`.
/// Rewriting this as `or_insert` looks equivalent and silently breaks the
/// hierarchy: every layer below the client would stop applying to any key
/// the client happened to send.
///
/// # Client trust
///
/// `layers.trust_client_sampling` gates which of the client's own fields
/// enter that layer at all. When `false` (the default — see
/// `Settings::trust_client_sampling`), only `max_tokens` survives; the rest
/// of `body`'s sampling keys are read but discarded before the fold, so a
/// client with a hardcoded `temperature` can no longer outrank this
/// server's own configuration, and every field it left unset still
/// gap-fills from below exactly as if it had never sent that key.
///
/// The gate covers modelled fields; sampler keys the ladder has no field
/// for ([`UNMODELLED_SAMPLER_KEYS`]) are stripped from the untrusted body
/// itself, because a key with no layer has nothing to be discarded from and
/// would otherwise ride the body to llama-server ungoverned.
///
/// A body that is not a JSON object is left alone.
/// Read the client's own sampling parameters and apply the trust gate.
///
/// Returns the layer to fold, what could not be read, and what the gate
/// dropped. Both lists are reported rather than swallowed: between them they
/// are every way a value the client actually sent can fail to reach
/// llama-server from this stage, and until recently neither was visible.
fn read_client_layer(
    body: &Value,
    trust_client_sampling: bool,
) -> (InferenceConfig, Vec<FieldIssue>, Vec<String>) {
    let (client_params, issues) = InferenceConfig::extract_client_sampling(body);
    if !issues.is_empty() {
        // Not `warn!`: a client sending a field gglib cannot read is a fact
        // about that client, not a fault in this server, and on the busiest
        // path in the system a warning per request would be noise. It is
        // recorded rather than swallowed because until now it was neither —
        // one unreadable field discarded the client's whole sampling layer
        // with nothing said.
        debug!(
            issues = %issues.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
            "client sampling: some fields were not usable as sent"
        );
    }

    // `max_tokens` stays client-authoritative regardless of trust: it is a
    // budget, not a taste, and dropping it would silently truncate the
    // client's own turns. See `Settings::trust_client_sampling`.
    if trust_client_sampling {
        return (client_params, issues, Vec::new());
    }

    // What the gate is about to bin. This is the default posture and the
    // highest-volume path in the system, so it is the largest silent discard
    // gglib performs — a sustained non-empty list here says clients are
    // trying to steer sampling and are being overruled, which an operator may
    // well want to know.
    let discarded: Vec<String> = client_params
        .to_openai_json_patch()
        .into_iter()
        .map(|(k, _)| k)
        .filter(|k| k != "max_tokens")
        .collect();
    if !discarded.is_empty() {
        debug!(
            discarded = %discarded.join(", "),
            "client sampling: untrusted, dropping all but max_tokens"
        );
    }

    let gated = InferenceConfig {
        max_tokens: client_params.max_tokens,
        ..InferenceConfig::default()
    };
    (gated, issues, discarded)
}

/// Sampler-taste keys llama-server reads that [`InferenceConfig`] does not
/// model, stripped from an untrusted body by
/// [`strip_unmodelled_sampler_keys`].
///
/// The trust gate discards the client's sampling *layer*, but the resolved
/// patch is only ever **inserted** into the body — nothing removed the keys
/// the ladder has no field for. So every key here was a way for an untrusted
/// client to steer sampling past the gate: gglib's own values arrived intact,
/// the readback saw no divergence (`/slots.params` echoes what was parsed,
/// not what the chain did — ADR 0003 finding 7), and the applied chain was
/// something nobody configured. `mirostat` alone replaces the entire
/// truncation stack.
///
/// Scope: **taste, not function**. Budgets (`max_tokens`), stops, constraint
/// machinery (`grammar`, `json_schema`, `response_format`) and observation
/// (`n_probs`, `logprobs`) stay client-authoritative — they say what the
/// request *is*, not how it should sample. `logit_bias` stays too, a
/// deliberate edge: it is per-token surgery with legitimate functional uses
/// (banning a token), and a dedicated decision should move it, not a sweep.
///
/// A modelled key must never appear here — the gate already governs those,
/// and stripping one would delete the client's value *before* the trusted
/// path could read it. `no_modelled_key_is_listed_as_unmodelled` pins this,
/// so modelling a new parameter (as `frequency_penalty` just was) forces its
/// removal from this list.
const UNMODELLED_SAMPLER_KEYS: &[&str] = &[
    "typical_p",
    "xtc_probability",
    "xtc_threshold",
    "mirostat",
    "mirostat_tau",
    "mirostat_eta",
    "dry_sequence_breakers",
    "repeat_last_n",
    "samplers",
    "min_keep",
];

/// Remove [`UNMODELLED_SAMPLER_KEYS`] from an untrusted body, returning what
/// was removed so it joins the discard record.
///
/// A no-op when the client is trusted — trusted means trusted, unmodelled
/// keys included — and on a body that is not a JSON object, which the rest of
/// the pipeline also leaves alone.
fn strip_unmodelled_sampler_keys(body: &mut Value, trust_client_sampling: bool) -> Vec<String> {
    if trust_client_sampling {
        return Vec::new();
    }
    let Some(obj) = body.as_object_mut() else {
        return Vec::new();
    };
    let mut stripped = Vec::new();
    for key in UNMODELLED_SAMPLER_KEYS {
        if obj.remove(*key).is_some() {
            stripped.push((*key).to_owned());
        }
    }
    if !stripped.is_empty() {
        debug!(
            stripped = %stripped.join(", "),
            "client sampling: untrusted, stripping unmodelled sampler keys"
        );
    }
    stripped
}

/// Which rung the model's stored defaults occupy, and the name that rung
/// carries — both decided by [`DefaultsOrigin`] in one place, so resolution
/// and its labelling cannot disagree.
///
/// Exhaustive on purpose — see the twin in `resolve_with_profile_explained`.
/// A catch-all here would rank any future unreviewed origin above global
/// settings, which is precisely backwards.
///
/// The name was the static `"model (auto-detected)"`, which lied in the
/// debug line and the audit's provenance strings whenever the occupant was a
/// published recipe — and would have credited gglib's guess for a tune
/// sweep's winner the same way.
const fn model_rung(
    ctx: &ModelContext,
) -> (
    Option<&InferenceConfig>,
    Option<&InferenceConfig>,
    &'static str,
) {
    match ctx.defaults_origin {
        Some(DefaultsOrigin::AutoDetected) => (
            None,
            ctx.inference_defaults.as_ref(),
            "model (auto-detected)",
        ),
        Some(DefaultsOrigin::Published) => {
            (None, ctx.inference_defaults.as_ref(), "model (published)")
        }
        Some(DefaultsOrigin::Measured) => {
            (None, ctx.inference_defaults.as_ref(), "model (measured)")
        }
        Some(DefaultsOrigin::User) | None => (
            ctx.inference_defaults.as_ref(),
            None,
            "model (auto-detected)",
        ),
    }
}

pub fn resolve_sampling(
    body: &mut Value,
    ctx: &ModelContext,
    layers: &SamplingLayers,
) -> SamplingDecision {
    let (client_layer, issues, mut discarded) =
        read_client_layer(body, layers.trust_client_sampling);

    // Keys the ladder cannot govern get no layer to lose in — they would ride
    // the body straight to llama-server, past the gate that just ran. Strip
    // them here, before the fold, so the discard record carries the whole of
    // what an untrusted client asked for and did not get.
    discarded.extend(strip_unmodelled_sampler_keys(
        body,
        layers.trust_client_sampling,
    ));

    // The gate's own drops leave the body too. Force-insert only overwrites
    // keys the resolution actually emits, and since ADR 0003 six modelled
    // fields resolve to nothing by design — so a gated key the ladder then
    // stays silent on would ride the body to llama-server exactly like an
    // unmodelled one. Found live, not by review: an untrusted client's
    // frequency_penalty: 0.9 reached /slots intact, because no layer names
    // that field and nothing overwrote it. Before the deferral this could
    // not happen — the floor emitted every modelled key — which is why the
    // gate never needed this until now.
    if let Some(obj) = body.as_object_mut() {
        for key in &discarded {
            obj.remove(key.as_str());
        }
    }

    // The `reasoning` tag selects the floor beneath every layer here — a
    // model that degrades into repetitive loops under greedy decoding still
    // gets a real anti-repetition guard when nothing above the floor sets
    // one, rather than the universal neutral default. See
    // `InferenceConfig::reasoning_floor`.
    let model_is_reasoning = crate::domain::capability_tags::is_reasoning(&ctx.tags);
    let floor = if model_is_reasoning {
        InferenceConfig::reasoning_floor()
    } else {
        InferenceConfig::with_hardcoded_defaults()
    };

    // Whether an agentic-turn temperature ceiling is eligible to apply. Only
    // eligibility — whether it actually bites depends on where the resolved
    // temperature came from, which is not known until after the fold.
    //
    // Keyed on tools being present, not on `tool_choice: "required"`: agentic
    // clients send `"auto"` almost universally, so a `required`-only trigger
    // would describe nearly no real traffic. See `request_shape::carries_tools`.
    //
    // Stage 2b has already removed `tools` for a model that cannot call them,
    // so this cannot fire on a model that would never emit a tool call —
    // except on a passthrough context, where nothing is known about the model
    // and nothing was stripped.
    let agentic_turn = layers.agentic_adjustments
        && !agentic_sampling_disabled_via_env()
        && super::request_shape::carries_tools(body);

    // `model` occupies one of two rungs depending on how it was set — never
    // both — so an auto-detected guess can't silently outrank global
    // settings the way a deliberate per-model choice should. See
    // `DefaultsOrigin` and `InferenceConfig::resolve_with_profile`.
    let (user_model, auto_model, below_global_rung_name) = model_rung(ctx);

    // Highest priority first. The single ordering both resolution and
    // provenance reporting read from, so they can never drift apart.
    let ordered: [(&str, Option<&InferenceConfig>); 6] = [
        ("cli", layers.cli_override.as_ref()),
        ("client", Some(&client_layer)),
        ("profile", layers.profile.as_ref()),
        ("model", user_model),
        ("global", layers.global.as_ref()),
        (below_global_rung_name, auto_model),
    ];
    let layer_configs: Vec<Option<&InferenceConfig>> =
        ordered.iter().map(|(_, config)| *config).collect();
    // Values and provenance come from the same pass over the same ladder, so
    // the log can never name a layer the resolution did not use.
    let (mut resolved, sources) =
        InferenceConfig::resolve_layers_with_sources(&layer_configs, &floor);

    // Cap the temperature for an agentic turn — but only over a value nobody
    // deliberately chose.
    //
    // The gate is provenance, not rank. A ladder rung would have been the
    // obvious way to express "outranks the auto-detected recipe", and it is
    // wrong: a rung that names a `temperature` *claims the coupled trio* under
    // `resolve_layers_with_sources`, so `presence_penalty`, `repeat_penalty` and `min_p`
    // would drop to the floor behind it. A `reasoning` model would silently
    // lose the 1.5 presence penalty its own recipe pairs with its temperature
    // on every agentic turn. Clamping after the fold leaves the trio
    // untouched.
    //
    // Eligible sources are the auto-detected rung and the floor. An
    // auto-detected recipe is an unreviewed guess written at import time, and
    // already ranks below global settings for that reason; a task-aware
    // ceiling outranking it is consistent with that. Anything a person
    // actually set — cli, client, profile, per-model, global — stands.
    //
    // Reasoning models have no ceiling at all — that is a measured decision,
    // not an omission; see `agentic_temperature_ceiling` for the experiment
    // that removed it (tune runs #12–#32) and ADR 0004's postscript.
    let auto_detected_rung = ordered.len() - 1;
    // A measured recipe is the one below-global origin the ceiling defers
    // to. The tune sweep resolved its candidates against this model's real
    // context (#748) precisely so the winner transfers to production —
    // capping the stored winner here would un-measure it on exactly the
    // turns it was measured for. Only the *model rung* is exempted: a
    // Measured model whose recipe names no temperature still resolves from
    // the floor, and the floor stays cappable — nobody measured the floor.
    let measured_model_rung = matches!(ctx.defaults_origin, Some(DefaultsOrigin::Measured))
        && sources.temperature == ParamSource::Layer(auto_detected_rung);
    let temperature_is_unchosen =
        !sources.temperature.is_deliberate_choice(auto_detected_rung) && !measured_model_rung;
    let applied_ceiling =
        InferenceConfig::agentic_temperature_ceiling(model_is_reasoning).filter(|&ceiling| {
            agentic_turn
                && temperature_is_unchosen
                && resolved.temperature.is_some_and(|t| t > ceiling)
        });
    if let Some(ceiling) = applied_ceiling {
        resolved.temperature = Some(ceiling);
    }

    if tracing::enabled!(tracing::Level::DEBUG) {
        let names: Vec<&str> = ordered.iter().map(|(name, _)| *name).collect();
        debug!(
            temperature = ?resolved.temperature,
            top_p = ?resolved.top_p,
            top_k = ?resolved.top_k,
            max_tokens = ?resolved.max_tokens,
            presence_penalty = ?resolved.presence_penalty,
            repeat_penalty = ?resolved.repeat_penalty,
            min_p = ?resolved.min_p,
            frequency_penalty = ?resolved.frequency_penalty,
            dynatemp_range = ?resolved.dynatemp_range,
            dynatemp_exponent = ?resolved.dynatemp_exponent,
            top_n_sigma = ?resolved.top_n_sigma,
            dry_multiplier = ?resolved.dry_multiplier,
            dry_base = ?resolved.dry_base,
            dry_allowed_length = ?resolved.dry_allowed_length,
            dry_penalty_last_n = ?resolved.dry_penalty_last_n,
            from = %sources.describe(&names),
            // Which class floor was used. `sources` says a value came from
            // "floor" but not which one, and the explain surfaces cannot show
            // this at all — they resolve stored configuration with no request
            // in hand.
            floor = if model_is_reasoning { "reasoning" } else { "default" },
            // Reported separately from `from`, which still names the rung the
            // temperature *would* have come from. The ceiling does not replace
            // that rung, it caps what it supplied.
            agentic_turn,
            agentic_ceiling = applied_ceiling,
            "sampling resolved"
        );
    }

    let layer_names: [&'static str; LADDER_RUNGS] = ordered.map(|(name, _)| name);
    let decision = |applied| SamplingDecision {
        resolved: resolved.clone(),
        sources,
        layer_names,
        floor: if model_is_reasoning {
            FloorClass::Reasoning
        } else {
            FloorClass::Default
        },
        agentic_turn,
        agentic_ceiling_applied: applied_ceiling,
        client_fields_rejected: issues.clone(),
        client_fields_discarded: discarded.clone(),
        applied,
    };

    let Some(obj) = body.as_object_mut() else {
        return decision(false);
    };

    for (key, value) in resolved.to_openai_json_patch() {
        obj.insert(key, value);
    }

    // Force-insert (not or_insert) llama-server's own `cache_prompt` flag.
    // It defaults to true server-side, but nothing guarantees the calling
    // client doesn't send `false` — and if it ever did, llama-server's
    // n_past = get_common_prefix(...) reuse computation (server-context.cpp)
    // is skipped entirely, silently discarding 100% of any restored/hot KV
    // state and forcing a full re-prefill regardless of how well the prompt
    // actually matches. The whole KV cache session persistence feature depends
    // on this staying true, so pin it rather than trusting it implicitly.
    obj.insert("cache_prompt".to_owned(), Value::Bool(true));

    decision(true)
}

#[cfg(test)]
#[path = "sampling_tests.rs"]
mod sampling_tests;
