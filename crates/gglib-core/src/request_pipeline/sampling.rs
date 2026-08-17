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
use crate::domain::inference::{
    REASONING_BUDGET_TOKENS_KEY, REASONING_EFFORT_KEY, THINKING_BUDGET_TOKENS_KEY,
};
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
    /// everything the client sent except [`CLIENT_AUTHORITATIVE_KEYS`] — the
    /// client's own *budgets*, currently `max_tokens` and
    /// `reasoning_budget_tokens` — see the field doc on `Settings` for why,
    /// and that constant for what makes a key a budget. This is read from the
    /// same settings snapshot as [`Self::global`], which is why it lives here
    /// rather than as a separate parameter threaded through every caller.
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
        .filter(|k| !CLIENT_AUTHORITATIVE_KEYS.contains(&k.as_str()))
        .collect();
    if !discarded.is_empty() {
        debug!(
            discarded = %discarded.join(", "),
            kept = %CLIENT_AUTHORITATIVE_KEYS.join(", "),
            "client sampling: untrusted, dropping all but the client's own budgets"
        );
    }

    // The carve-out, and it is exactly `CLIENT_AUTHORITATIVE_KEYS` — kept in
    // sync by `every_client_authoritative_key_survives_an_untrusted_request`,
    // because the list above governs the *discard record* and this struct
    // governs what is actually kept, and a field in one and not the other is
    // either a silent drop or an unreported survival.
    let gated = InferenceConfig {
        max_tokens: client_params.max_tokens,
        reasoning_budget_tokens: client_params.reasoning_budget_tokens,
        ..InferenceConfig::default()
    };
    (gated, issues, discarded)
}

/// The client's own fields that survive an untrusted request.
///
/// # Budgets, not tastes — and the reasoning pair splits along that line
///
/// `max_tokens` has always been here: it is a budget on the client's own turn,
/// and dropping it would silently truncate answers the client sized
/// deliberately. `UNMODELLED_SAMPLER_KEYS`' own scope note draws the rule —
/// "Budgets (`max_tokens`), stops, constraint machinery and observation ...
/// stay client-authoritative — they say what the request *is*, not how it
/// should sample."
///
/// `reasoning_budget_tokens` is that category by name, so it joins. It caps
/// how many tokens this turn may spend thinking, it is enforced by llama.cpp's
/// own sampler-side budget rather than by a template, and — the load-bearing
/// half — **upstream governs it**: `-2` comes back as an HTTP 400 naming the
/// range ([ADR 0007] finding 7c). A client sending it is asking for a shape of
/// turn, within bounds a second system already enforces.
///
/// `reasoning_effort` does **not** join, and the asymmetry runs the opposite
/// way to what its name suggests. It is taste: it steers what the model is
/// shown, its level vocabulary is per-template folklore, and upstream
/// validates it *not at all* — `"banana"` is accepted and rendered into the
/// prompt verbatim. So it is precisely the field where an untrusted client's
/// value would reach the model unexamined by anyone, which is what the trust
/// gate exists to stop. When untrusted it is dropped from the client layer,
/// removed from the body by the cleanup in [`resolve_sampling`], and named in
/// `client_fields_discarded`.
///
/// The gate only reaches a level gglib could *read*, so it is half the story:
/// an unreadable one (`"banana"`, `"none"`) never becomes a layer value to
/// discard. That half is the same cleanup's `issues` arm, and it applies on
/// both sides of the gate — trusting a client is not trusting a typo.
///
/// Neither control is observable afterwards (ADR 0007 finding 7a), so the
/// discard record is the only place the decision is ever visible.
///
/// # Public because the operator-facing surfaces describe it
///
/// `gglib model explain` and the GUI's sampling inspector both print a caveat
/// naming what survives an untrusted request, because the client rung is a
/// real rung neither table can show. That sentence read "except `max_tokens`"
/// for as long as this list was one key long, and nothing would have failed
/// had it stayed that way after `reasoning_budget_tokens` joined — a
/// user-facing description of the trust boundary, silently false. Exporting
/// the list lets `caveats_name_every_client_authoritative_key` in
/// `gglib-cli`'s `explain_display` assert the sentence against it, so the
/// next key added here fails a test instead of shipping a wrong caveat. The
/// TypeScript half cannot read a Rust constant; it carries its own copy,
/// named and pinned, with a pointer back here.
///
/// [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md
pub const CLIENT_AUTHORITATIVE_KEYS: &[&str] = &["max_tokens", REASONING_BUDGET_TOKENS_KEY];

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

/// Remove from the body the client keys gglib must not forward: what the trust
/// gate binned, the budget alias gglib never emits, and one refused field.
///
/// The single place body keys leave this stage, so "what does gglib delete
/// from a request" has one answer in one function.
///
/// # `discarded` — what the trust gate binned
///
/// The resolved patch is only ever *inserted*, and since ADR 0003 six modelled
/// fields resolve to nothing by design, so a gated key the ladder then stays
/// silent on rides the body to llama-server exactly like an unmodelled one.
/// Found live, not by review: an untrusted client's `frequency_penalty: 0.9`
/// reached `/slots` intact, because no layer names that field and nothing
/// overwrote it. Before the deferral this could not happen — the floor emitted
/// every modelled key — which is why the gate never needed this until then.
///
/// Empty when the client is trusted.
///
/// # The budget alias — always, whatever the trust setting
///
/// llama-server reads [`THINKING_BUDGET_TOKENS_KEY`] as a second spelling of
/// `reasoning_budget_tokens` (ADR 0007 finding 7c). gglib reads it too, so its
/// value is already *in* the resolved ladder — but gglib emits the canonical
/// key only, and a surviving alias is a second answer to the same question
/// sitting next to the force-inserted first. Which one wins would then be
/// llama-server's parse order rather than gglib's ladder. Removing it is not a
/// trust decision, it is a consequence of gglib having one canonical spelling.
///
/// # `issues` — one field, and the asymmetry is upstream's
///
/// A refused value never becomes `Some`, so it never enters the resolved patch
/// and never entered `discarded` either: the rejection stops at the layer, and
/// the client's own text rides on. For nearly every field that is exactly
/// right — these readers reject what llama-server rejects, so the forwarded
/// value earns a clean HTTP 400 from the system that owns the field, which
/// tells the client more than a silent substitution would and keeps gglib no
/// stricter than upstream (the doctrine on
/// [`InferenceConfig::extract_client_sampling`]).
///
/// ADR 0007 finding 7c measured where that stops holding. Upstream **governs
/// the budget** — `reasoning_budget_tokens: -2` comes back a 400 naming the
/// range — and **does not govern effort at all**. So the two reasoning
/// controls split:
///
/// - [`REASONING_EFFORT_KEY`], refused → **deleted**. There is no downstream
///   400 to inherit: `"banana"` is accepted upstream and rendered into the
///   user's prompt verbatim. Left in the body, gglib's refusal would be a
///   record in `client_fields_rejected` of a value the model then read. This
///   is the one field where gglib's "no" has to be the only "no" there is.
/// - [`REASONING_BUDGET_TOKENS_KEY`], refused → **left in place**, like every
///   other field. The client gets upstream's honest 400. And if the ladder
///   resolves a budget of its own, the force-insert overwrites the client's
///   text before it is ever sent, so the refusal costs nothing.
///
/// A [`FieldIssue::Normalised`] deletes nothing under either rule: the
/// substitute is either force-inserted over the client's spelling or is an
/// absence that llama.cpp reads from the client's own sentinel anyway
/// (`max_tokens: -1`).
///
/// A body that is not a JSON object is left alone, as everywhere else here.
fn erase_unadopted_client_keys(body: &mut Value, discarded: &[String], issues: &[FieldIssue]) {
    let Some(obj) = body.as_object_mut() else {
        return;
    };
    for key in discarded.iter().map(String::as_str) {
        obj.remove(key);
    }
    obj.remove(THINKING_BUDGET_TOKENS_KEY);
    let effort_refused = issues.iter().any(|issue| {
        matches!(issue, FieldIssue::Rejected { field, .. } if *field == REASONING_EFFORT_KEY)
    });
    if effort_refused {
        obj.remove(REASONING_EFFORT_KEY);
    }
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

/// Resolve the sampling hierarchy into `body`, then pin `cache_prompt`.
///
/// This doc block used to sit above `read_client_layer`, where a split left
/// it fused to that function's own first line — so the entry point of the
/// whole stage was undocumented while a private helper carried a description
/// of something else. Restored here; `read_client_layer` keeps its own.
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
/// `Settings::trust_client_sampling`), only [`CLIENT_AUTHORITATIVE_KEYS`]
/// survive; the rest of `body`'s sampling keys are read but discarded before
/// the fold, so a client with a hardcoded `temperature` can no longer outrank
/// this server's own configuration, and every field it left unset still
/// gap-fills from below exactly as if it had never sent that key.
///
/// That carve-out is a *category*, not a list of exceptions, and the two
/// reasoning controls land on opposite sides of it: the budget is a budget and
/// survives, the effort level is taste and does not. The reasoning is on
/// [`CLIENT_AUTHORITATIVE_KEYS`].
///
/// The gate covers modelled fields; sampler keys the ladder has no field
/// for (`UNMODELLED_SAMPLER_KEYS`) are stripped from the untrusted body
/// itself, because a key with no layer has nothing to be discarded from and
/// would otherwise ride the body to llama-server ungoverned.
///
/// # What leaves the body
///
/// `erase_unadopted_client_keys` is the one place keys are deleted, and it
/// deletes three things — only the first of which is about trust:
///
/// - **What the gate binned** — empty when the client is trusted.
/// - **`thinking_budget_tokens`** — always. It is upstream's alias for the
///   budget, gglib reads it and then emits the canonical key alone, and two
///   spellings of one parameter in one body is a disagreement waiting to be
///   resolved by somebody else's parse order.
/// - **A refused `reasoning_effort`** — on *both* sides of the gate, and it
///   is the only field an `issues` entry removes. Every other refused value
///   is forwarded exactly as before this PR, because upstream 400s on it and
///   that 400 is a better answer to the client than gglib quietly rewriting
///   the request. `reasoning_effort` is the exception because upstream
///   validates it not at all: a refused `"banana"` left in the body is not
///   rejected downstream, it is rendered into the prompt.
///
/// So a client's `top_k: "5"` still reaches llama-server and still earns its
/// HTTP 400, unchanged by the reasoning work. The helper carries the full
/// argument and ADR 0007's finding behind it.
///
/// A body that is not a JSON object is left alone.
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

    // Runs before the fold, so any key the ladder does resolve is re-inserted
    // below with gglib's own value. Narrow on purpose: the gate's drops, the
    // budget alias gglib never emits, and a refused `reasoning_effort` — every
    // other unreadable value is left for llama-server to answer, as it always
    // was.
    erase_unadopted_client_keys(body, &discarded, &issues);

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

    // Nothing is logged here. The whole decision is rendered once, by
    // `super::sampling_log`, after stage 5b — which can still delete a resolved
    // `reasoning_effort` and would leave this line stating that gglib sent one.
    // See that module for why a second, correcting line was not good enough.
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
