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
use crate::domain::{DefaultsOrigin, InferenceConfig, ParamSource};

/// The sampling layers that sit *below* the client's own request parameters.
///
/// Grouped because they are only ever used together, at the single point where
/// [`resolve_sampling`] folds them through [`InferenceConfig::resolve_layers`].
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
    std::env::var(DISABLE_AGENTIC_SAMPLING_ENV)
        .ok()
        .is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

/// Resolve the sampling hierarchy into `body`, then pin `cache_prompt`.
///
/// # Force-insert, not `or_insert`
///
/// The client's own parameters are extracted from `body` first, folded
/// through [`InferenceConfig::resolve_layers`] alongside cli / profile /
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
/// A body that is not a JSON object is left alone.
pub fn resolve_sampling(body: &mut Value, ctx: &ModelContext, layers: &SamplingLayers) {
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
    let client_layer = if layers.trust_client_sampling {
        client_params
    } else {
        // What the gate is about to bin. This is the default posture and the
        // highest-volume path in the system, so it is the largest silent
        // discard gglib performs — a sustained non-empty list here says
        // clients are trying to steer sampling and are being overruled,
        // which an operator may well want to know.
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
        InferenceConfig {
            max_tokens: client_params.max_tokens,
            ..InferenceConfig::default()
        }
    };

    // The `reasoning` tag selects the floor beneath every layer here — a
    // model that degrades into repetitive loops under greedy decoding still
    // gets a real anti-repetition guard when nothing above the floor sets
    // one, rather than the universal neutral default. See
    // `InferenceConfig::reasoning_floor`.
    let model_is_reasoning = ctx
        .tags
        .iter()
        .any(|tag| tag.eq_ignore_ascii_case("reasoning"));
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
    let (user_model, auto_model) = match ctx.defaults_origin {
        Some(DefaultsOrigin::AutoDetected) => (None, ctx.inference_defaults.as_ref()),
        _ => (ctx.inference_defaults.as_ref(), None),
    };

    // Highest priority first. The single ordering both resolution and
    // provenance reporting read from, so they can never drift apart.
    let ordered: [(&str, Option<&InferenceConfig>); 6] = [
        ("cli", layers.cli_override.as_ref()),
        ("client", Some(&client_layer)),
        ("profile", layers.profile.as_ref()),
        ("model", user_model),
        ("global", layers.global.as_ref()),
        ("model (auto-detected)", auto_model),
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
    // `resolve_layers`, so `presence_penalty`, `repeat_penalty` and `min_p`
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
    let ceiling = InferenceConfig::agentic_temperature_ceiling(model_is_reasoning);
    let auto_detected_rung = ordered.len() - 1;
    let temperature_is_unchosen = matches!(
        sources.temperature,
        ParamSource::Floor | ParamSource::FloorCoupled | ParamSource::Unset
    ) || sources.temperature
        == ParamSource::Layer(auto_detected_rung);
    let ceiling_applied = agentic_turn
        && temperature_is_unchosen
        && resolved.temperature.is_some_and(|t| t > ceiling);
    if ceiling_applied {
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
            agentic_ceiling = ceiling_applied.then_some(ceiling),
            "sampling resolved"
        );
    }

    let Some(obj) = body.as_object_mut() else {
        return;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(value: f32) -> InferenceConfig {
        InferenceConfig {
            temperature: Some(value),
            ..Default::default()
        }
    }

    fn model_ctx(defaults: Option<InferenceConfig>) -> ModelContext {
        ModelContext {
            inference_defaults: defaults,
            ..ModelContext::passthrough()
        }
    }

    /// `f32 → f64` widening makes exact literal comparison unreliable.
    #[track_caller]
    fn assert_param(body: &Value, key: &str, expected: f64) {
        let actual = body
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("{key} missing from body: {body}"));
        assert!(
            (actual - expected).abs() < 1e-6,
            "{key}: expected {expected}, got {actual}"
        );
    }

    // ── The hierarchy ─────────────────────────────────────────────────────

    /// One table, one row per layer: each wins only over the ones beneath it.
    #[test]
    fn each_layer_beats_the_ones_below_it() {
        let cases = [
            // (cli, client temperature, profile, model, global, expected, why)
            (
                Some(0.05),
                Some(0.11),
                Some(0.22),
                Some(0.33),
                Some(0.44),
                0.05,
                "cli override beats client",
            ),
            (
                None,
                Some(0.11),
                Some(0.22),
                Some(0.33),
                Some(0.44),
                0.11,
                "client beats profile",
            ),
            (
                None,
                None,
                Some(0.22),
                Some(0.33),
                Some(0.44),
                0.22,
                "profile beats model",
            ),
            (
                None,
                None,
                None,
                Some(0.33),
                Some(0.44),
                0.33,
                "model beats global",
            ),
            (
                None,
                None,
                None,
                None,
                Some(0.44),
                0.44,
                "global beats hardcoded",
            ),
            (None, None, None, None, None, 0.7, "hardcoded fallback"),
        ];

        for (cli, client, profile, model, global, expected, why) in cases {
            let mut body = client.map_or_else(|| json!({}), |t| json!({"temperature": t}));
            let layers = SamplingLayers {
                cli_override: cli.map(temp),
                profile: profile.map(temp),
                global: global.map(temp),
                // This table is specifically about layer precedence, not
                // about the trust gate — trust it here so the "client beats
                // profile" / "cli beats client" rows still exercise the
                // client layer at all. See `client_sampling_is_ignored_by_default`.
                trust_client_sampling: true,
                agentic_adjustments: false,
            };
            resolve_sampling(&mut body, &model_ctx(model.map(temp)), &layers);
            assert_param(&body, "temperature", expected);
            assert!(
                body["temperature"].as_f64().is_some(),
                "{why}: temperature must be present"
            );
        }
    }

    /// Profiles are sparse: outranking the model layer must not blank out the
    /// untuned parameters the profile says nothing about.
    #[test]
    fn a_sparse_profile_leaves_other_model_defaults_intact() {
        let mut body = json!({});
        let model = InferenceConfig {
            temperature: Some(1.0),
            top_p: Some(0.87),
            top_k: Some(20),
            ..Default::default()
        };
        resolve_sampling(
            &mut body,
            &model_ctx(Some(model)),
            &SamplingLayers {
                cli_override: None,
                profile: Some(temp(0.2)),
                global: None,
                trust_client_sampling: false,
                agentic_adjustments: false,
            },
        );

        assert_param(&body, "temperature", 0.2);
        assert_param(&body, "top_p", 0.87);
        assert_param(&body, "top_k", 20.0);
    }

    // ── Provenance ────────────────────────────────────────────────────────

    /// The six names the pipeline's own ladder uses, for the provenance
    /// tests below.
    ///
    /// Six, not five. This helper was 5-wide while `resolve_sampling` built a
    /// 6-rung ladder, so nothing exercised the real index→name mapping and
    /// three separate doc comments drifted to three different rung counts
    /// before anyone noticed. Keep it the same width as the array at the top
    /// of `resolve_sampling` or it stops testing the thing it looks like it
    /// tests.
    const LAYER_NAMES: [&str; 6] = [
        "cli",
        "client",
        "profile",
        "model",
        "global",
        "model (auto-detected)",
    ];

    /// Resolve a ladder and render its provenance the way the debug line does.
    fn provenance_of(layers: &[Option<&InferenceConfig>; 6]) -> String {
        let floor = InferenceConfig::with_hardcoded_defaults();
        InferenceConfig::resolve_layers_with_sources(layers, &floor)
            .1
            .describe(&LAYER_NAMES)
    }

    /// The `:coding` shape. The provenance must say the penalty came from the
    /// floor, not from the model — otherwise the log would assert exactly the
    /// leak the merge now prevents.
    #[test]
    fn provenance_reports_coupling_suppressed_layers_as_floor() {
        let model = InferenceConfig {
            temperature: Some(1.0),
            presence_penalty: Some(1.5),
            top_k: Some(20),
            ..Default::default()
        };
        let profile = temp(0.2);
        let got = provenance_of(&[None, None, Some(&profile), Some(&model), None, None]);

        assert!(got.contains("temperature=profile"), "{got}");
        assert!(got.contains("presence_penalty=floor"), "{got}");
        // Untuned parameters are unaffected by the claim.
        assert!(got.contains("top_k=model"), "{got}");
    }

    /// With nothing above it claiming a temperature, the model's own recipe is
    /// reported intact.
    #[test]
    fn provenance_attributes_an_unclaimed_recipe_to_the_model() {
        let model = InferenceConfig {
            temperature: Some(1.0),
            presence_penalty: Some(1.5),
            ..Default::default()
        };
        let got = provenance_of(&[None, None, None, Some(&model), None, None]);

        assert!(got.contains("temperature=model"), "{got}");
        assert!(got.contains("presence_penalty=model"), "{got}");
    }

    /// Operator flags are reported as their own layer, above the client.
    #[test]
    fn provenance_names_the_cli_layer() {
        let cli = temp(0.3);
        let client = temp(0.9);
        let got = provenance_of(&[Some(&cli), Some(&client), None, None, None, None]);

        assert!(got.contains("temperature=cli"), "{got}");
    }

    /// The drift this unification removes. `cli` names a `presence_penalty`
    /// but no `temperature`; `model` claims the temperature, so the coupling
    /// rule resolves the penalty from `model` — and the provenance must say so.
    ///
    /// The previous `describe_provenance` scanned every layer down to the
    /// claiming one and reported `cli`, naming a layer the resolution had
    /// passed over.
    #[test]
    fn provenance_does_not_credit_a_layer_the_coupling_rule_passed_over() {
        let cli = InferenceConfig {
            presence_penalty: Some(1.2),
            ..Default::default()
        };
        let model = InferenceConfig {
            temperature: Some(1.0),
            presence_penalty: Some(1.5),
            ..Default::default()
        };
        let layers = [Some(&cli), None, None, Some(&model), None, None];

        let got = provenance_of(&layers);
        assert!(
            got.contains("presence_penalty=model"),
            "the claiming layer supplied it, got: {got}"
        );

        // And the value agrees with the name.
        let floor = InferenceConfig::with_hardcoded_defaults();
        let (resolved, _) = InferenceConfig::resolve_layers_with_sources(&layers, &floor);
        assert_eq!(resolved.presence_penalty, Some(1.5));
    }

    /// Regression for #621: operator flags must beat the per-model layer.
    ///
    /// These previously merged into the *global* layer, which sits below the
    /// model — so on any model with stored `inference_defaults`, every
    /// `gglib proxy --temperature …` style flag silently did nothing.
    #[test]
    fn a_cli_override_beats_the_model_layer() {
        let mut body = json!({});
        resolve_sampling(
            &mut body,
            &model_ctx(Some(InferenceConfig {
                temperature: Some(1.0),
                top_k: Some(20),
                ..Default::default()
            })),
            &SamplingLayers {
                cli_override: Some(temp(0.3)),
                ..Default::default()
            },
        );

        assert_param(&body, "temperature", 0.3);
        // Untuned parameters the operator said nothing about still resolve.
        assert_param(&body, "top_k", 20.0);
    }

    /// The operator runs the server, so their flags also outrank the client's
    /// own request parameters — otherwise any caller could quietly ignore them.
    #[test]
    fn a_cli_override_beats_client_request_params() {
        let mut body = json!({"temperature": 0.9});
        resolve_sampling(
            &mut body,
            &model_ctx(None),
            &SamplingLayers {
                cli_override: Some(temp(0.3)),
                ..Default::default()
            },
        );

        assert_param(&body, "temperature", 0.3);
    }

    /// Regression for #621, at the pipeline level: the `:coding` shape — a
    /// profile that lowers the temperature — must not carry the model's
    /// `presence_penalty`, which was tuned for the model's own temperature.
    #[test]
    fn a_profile_temperature_does_not_carry_model_penalties() {
        let mut body = json!({});
        let model = InferenceConfig {
            temperature: Some(1.0),
            presence_penalty: Some(1.5),
            ..Default::default()
        };
        resolve_sampling(
            &mut body,
            &model_ctx(Some(model)),
            &SamplingLayers {
                cli_override: None,
                profile: Some(temp(0.2)),
                global: None,
                trust_client_sampling: false,
                agentic_adjustments: false,
            },
        );

        assert_param(&body, "temperature", 0.2);
        assert_param(&body, "presence_penalty", 0.0);
    }

    /// When the client IS trusted (`trust_client_sampling: true` — an
    /// `OpenWebUI`-style client with real sampling controls exposed to its
    /// user), a client that sends `temperature: 0` must still not silently
    /// zero out a reasoning model's only anti-repetition guard. The client
    /// still wins the temperature it asked for — it just doesn't also claim
    /// penalties it never named an opinion on. See `resolve_layers`'s
    /// coupling rule.
    #[test]
    fn trusted_client_temperature_zero_does_not_zero_a_reasoning_models_presence_penalty() {
        let mut body = json!({"temperature": 0.0});
        let ctx = ModelContext {
            tags: vec!["reasoning".to_owned()],
            inference_defaults: Some(InferenceConfig::reasoning_profile()),
            ..ModelContext::passthrough()
        };
        resolve_sampling(
            &mut body,
            &ctx,
            &SamplingLayers {
                trust_client_sampling: true,
                ..Default::default()
            },
        );

        assert_param(&body, "temperature", 0.0);
        assert_param(&body, "presence_penalty", 1.0);
    }

    /// Same as above, but a non-reasoning model gets the plain neutral floor,
    /// not the reasoning one — the class floor is opt-in via the tag, not a
    /// blanket change.
    #[test]
    fn trusted_client_temperature_zero_leaves_a_non_reasoning_model_at_the_neutral_floor() {
        let mut body = json!({"temperature": 0.0});
        let model = InferenceConfig {
            temperature: Some(0.8),
            presence_penalty: Some(0.6),
            ..Default::default()
        };
        resolve_sampling(
            &mut body,
            &model_ctx(Some(model)),
            &SamplingLayers {
                trust_client_sampling: true,
                ..Default::default()
            },
        );

        assert_param(&body, "temperature", 0.0);
        assert_param(&body, "presence_penalty", 0.0);
    }

    // ── Client sampling authority (Settings.trust_client_sampling) ─────────

    /// The default. This is the actual fix for the incident that motivated
    /// this whole refactor: without a client-trust escape hatch, a client
    /// hardcoding `temperature: 0` with no way for its user to change it (VS
    /// Code Copilot's LLM Gateway) claims the coupled set on every request
    /// and supplies none of it — so the model's own tuned recipe never has a
    /// chance to apply, no matter what `resolve_layers`'s coupling rule does.
    /// With the client out of the ladder entirely, the model's full recipe —
    /// temperature *and* the penalties tuned for it — resolves untouched.
    #[test]
    fn client_sampling_is_ignored_by_default() {
        let mut body = json!({"temperature": 0.0});
        let ctx = ModelContext {
            tags: vec!["reasoning".to_owned()],
            inference_defaults: Some(InferenceConfig::reasoning_profile()),
            ..ModelContext::passthrough()
        };
        resolve_sampling(&mut body, &ctx, &SamplingLayers::default());

        assert_param(&body, "temperature", 1.0); // the model's own, not the client's 0.0
        assert_param(&body, "presence_penalty", 1.5); // the model's tuned recipe, intact
    }

    /// `max_tokens` is a budget, not a taste — it stays client-authoritative
    /// even when nothing else about the client's request is trusted, because
    /// dropping it would silently truncate that client's own turns.
    #[test]
    fn max_tokens_is_still_honoured_when_client_sampling_is_untrusted() {
        let mut body = json!({"temperature": 0.9, "max_tokens": 999});
        resolve_sampling(
            &mut body,
            &model_ctx(Some(InferenceConfig {
                temperature: Some(0.4),
                ..Default::default()
            })),
            &SamplingLayers::default(),
        );

        assert_param(&body, "temperature", 0.4); // client's 0.9 dropped
        assert_param(&body, "max_tokens", 999.0); // client's budget still honoured
    }

    // ── Model defaults provenance (Model.defaults_origin) ───────────────────

    /// A user's own global settings must win over gglib's unreviewed guess —
    /// this is the actual regression this feature exists for. Without it, a
    /// `reasoning`-tagged model's auto-written recipe always wins over
    /// anything configured globally, with no way to tell the two apart in
    /// the resolved output.
    #[test]
    fn an_auto_detected_models_recipe_ranks_below_global_settings() {
        let mut body = json!({});
        let ctx = ModelContext {
            inference_defaults: Some(InferenceConfig::reasoning_profile()), // temp 1.0, presence 1.5
            defaults_origin: Some(DefaultsOrigin::AutoDetected),
            ..ModelContext::passthrough()
        };
        let layers = SamplingLayers {
            global: Some(InferenceConfig {
                temperature: Some(0.2),
                top_k: Some(20),
                min_p: Some(0.05),
                ..Default::default()
            }),
            ..Default::default()
        };
        resolve_sampling(&mut body, &ctx, &layers);

        assert_param(&body, "temperature", 0.2); // global beats the auto-detected guess
        assert_param(&body, "top_k", 20.0);
        assert_param(&body, "min_p", 0.05);
        // The claiming layer (global) left presence_penalty unset, so it
        // falls to the floor — never to the auto-detected model's 1.5,
        // which was tuned for a temperature global didn't choose.
        assert_param(&body, "presence_penalty", 0.0);
    }

    /// The same model, but with a deliberate per-model choice instead of an
    /// auto-detected one: it keeps outranking global settings exactly as
    /// before this feature existed — that is what "per-model" is supposed
    /// to mean.
    #[test]
    fn a_user_set_models_recipe_still_beats_global_settings() {
        let mut body = json!({});
        let ctx = ModelContext {
            inference_defaults: Some(InferenceConfig::reasoning_profile()),
            defaults_origin: Some(DefaultsOrigin::User),
            ..ModelContext::passthrough()
        };
        let layers = SamplingLayers {
            global: Some(InferenceConfig {
                temperature: Some(0.2),
                ..Default::default()
            }),
            ..Default::default()
        };
        resolve_sampling(&mut body, &ctx, &layers);

        assert_param(&body, "temperature", 1.0); // the user's own choice wins
        assert_param(&body, "presence_penalty", 1.5); // travels with it, intact
    }

    /// The force-insert. An `or_insert` implementation passes every test above
    /// and fails this one. Trusted explicitly: this test is about force-insert
    /// semantics, not about the trust gate.
    #[test]
    fn resolution_overwrites_a_partial_client_value_from_lower_layers() {
        // The client named only `temperature`. Every other key must still be
        // written from the layers beneath it rather than left absent.
        let mut body = json!({"temperature": 0.11});
        resolve_sampling(
            &mut body,
            &model_ctx(Some(InferenceConfig {
                top_p: Some(0.42),
                ..Default::default()
            })),
            &SamplingLayers {
                trust_client_sampling: true,
                ..Default::default()
            },
        );

        assert_param(&body, "temperature", 0.11);
        assert_param(&body, "top_p", 0.42);
    }

    // ── The agentic-turn temperature ceiling ──────────────────────────────

    fn tools_body() -> Value {
        json!({"tools": [{"function": {"name": "read_file"}}]})
    }

    fn agentic_layers() -> SamplingLayers {
        SamplingLayers {
            agentic_adjustments: true,
            ..Default::default()
        }
    }

    /// A model whose defaults were written automatically at import, which is
    /// what every `reasoning`-tagged model gets.
    fn auto_detected_ctx(defaults: InferenceConfig, reasoning: bool) -> ModelContext {
        ModelContext {
            tags: if reasoning {
                vec!["reasoning".to_owned()]
            } else {
                Vec::new()
            },
            inference_defaults: Some(defaults),
            defaults_origin: Some(DefaultsOrigin::AutoDetected),
            ..ModelContext::passthrough()
        }
    }

    /// The case the ceiling exists for, and the one the floor it replaced
    /// could never reach: a `reasoning` model's auto-written recipe names
    /// `temperature: 1.0`, and any layer outranks a floor.
    #[test]
    fn the_ceiling_caps_an_auto_detected_recipe() {
        let mut body = tools_body();
        let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
        resolve_sampling(&mut body, &ctx, &agentic_layers());

        assert_param(&body, "temperature", 0.6);
    }

    /// Reasoning models are capped far higher than everything else. Below
    /// ~0.6 they degrade into endless repetition, and the `<think>` block
    /// shares one sampler configuration with the tool call.
    #[test]
    fn the_reasoning_ceiling_is_higher_than_the_plain_one() {
        for (reasoning, expected) in [(true, 0.6), (false, 0.3)] {
            let mut body = tools_body();
            let ctx = auto_detected_ctx(
                InferenceConfig {
                    temperature: Some(1.0),
                    ..Default::default()
                },
                reasoning,
            );
            resolve_sampling(&mut body, &ctx, &agentic_layers());

            assert_param(&body, "temperature", expected);
        }
    }

    /// Deliberate configuration outranks the ceiling. This is the whole
    /// reason the gate is provenance rather than rank.
    #[test]
    fn a_deliberate_temperature_is_never_capped() {
        let mut body = tools_body();
        // `User` origin, so the recipe occupies the per-model rung rather than
        // the auto-detected one.
        let ctx = ModelContext {
            inference_defaults: Some(temp(0.9)),
            defaults_origin: Some(DefaultsOrigin::User),
            ..ModelContext::passthrough()
        };
        resolve_sampling(&mut body, &ctx, &agentic_layers());

        assert_param(&body, "temperature", 0.9);
    }

    /// The regression guard for #743: the adjustment must not disable DRY.
    #[test]
    fn dry_survives_an_agentic_turn() {
        let mut body = tools_body();
        let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
        resolve_sampling(
            &mut body,
            &ctx,
            &SamplingLayers {
                agentic_adjustments: true,
                global: Some(InferenceConfig {
                    temperature: Some(0.8),
                    dry_multiplier: Some(0.8),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        assert_param(&body, "dry_multiplier", 0.8);
        // Global is a deliberate setting, so its temperature stands uncapped.
        assert_param(&body, "temperature", 0.8);
    }

    /// Regression guard for #745. DRY is deliberately *not* part of the
    /// temperature-coupled trio, so a layer naming a DRY value but no
    /// temperature keeps it even though a lower layer claims the temperature —
    /// which is the default state of every `reasoning`-tagged model.
    #[test]
    fn dry_survives_without_a_temperature_in_the_same_layer() {
        let mut body = json!({});
        // The model's auto-detected recipe names a temperature, so it claims
        // the trio. The profile names only DRY.
        let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
        resolve_sampling(
            &mut body,
            &ctx,
            &SamplingLayers {
                profile: Some(InferenceConfig {
                    dry_multiplier: Some(0.8),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        assert_param(&body, "dry_multiplier", 0.8);
        // The trio still comes from the claiming layer, untouched by this.
        assert_param(&body, "presence_penalty", 1.5);
        assert_param(&body, "temperature", 1.0);
    }

    /// The ceiling only ever lowers. A model already below it is untouched.
    #[test]
    fn the_ceiling_never_raises_a_temperature() {
        let mut body = tools_body();
        let ctx = auto_detected_ctx(temp(0.1), false);
        resolve_sampling(&mut body, &ctx, &agentic_layers());

        assert_param(&body, "temperature", 0.1);
    }

    /// `top_p` is left alone. The floor this replaced forced it to 1.0, which
    /// contradicted Qwen's own guidance of 0.95 for thinking mode.
    #[test]
    fn the_ceiling_does_not_touch_top_p() {
        let mut body = tools_body();
        let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
        resolve_sampling(&mut body, &ctx, &agentic_layers());

        assert_param(&body, "top_p", 0.95);
    }

    #[test]
    fn a_request_without_tools_is_never_capped() {
        let mut body = json!({});
        let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
        resolve_sampling(&mut body, &ctx, &agentic_layers());

        assert_param(&body, "temperature", 1.0);
    }

    /// `strip_unsupported_tools` leaves a dangling `tool_choice` when there
    /// were no tools to strip, so this shape reaches stage 4 in practice.
    #[test]
    fn a_dangling_tool_choice_without_tools_is_not_an_agentic_turn() {
        let mut body = json!({"tool_choice": "required"});
        let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
        resolve_sampling(&mut body, &ctx, &agentic_layers());

        assert_param(&body, "temperature", 1.0);
    }

    #[test]
    fn the_ceiling_does_nothing_when_the_caller_has_not_enabled_it() {
        let mut body = tools_body();
        let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
        resolve_sampling(&mut body, &ctx, &SamplingLayers::default());

        assert_param(&body, "temperature", 1.0);
    }

    // ── cache_prompt ──────────────────────────────────────────────────────

    #[test]
    fn cache_prompt_is_pinned_true_when_absent() {
        let mut body = json!({});
        resolve_sampling(
            &mut body,
            &ModelContext::passthrough(),
            &SamplingLayers::default(),
        );
        assert_eq!(body["cache_prompt"], true);
    }

    /// The KV cache feature depends on this: a client that sends `false` must
    /// not be able to discard the whole restored cache.
    #[test]
    fn cache_prompt_is_forced_true_over_an_explicit_false() {
        let mut body = json!({"cache_prompt": false});
        resolve_sampling(
            &mut body,
            &ModelContext::passthrough(),
            &SamplingLayers::default(),
        );
        assert_eq!(body["cache_prompt"], true);
    }

    // ── Passthrough ───────────────────────────────────────────────────────

    #[test]
    fn unknown_fields_survive_untouched() {
        let mut body = json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}],
            "totally_made_up_key": {"nested": [1, 2, {"deep": true}]},
        });
        resolve_sampling(
            &mut body,
            &ModelContext::passthrough(),
            &SamplingLayers::default(),
        );

        assert_eq!(body["model"], "m");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(
            body["totally_made_up_key"],
            json!({"nested": [1, 2, {"deep": true}]})
        );
    }

    /// `max_tokens` has no hardcoded fallback on purpose — a value here would
    /// cap every request that did not name its own.
    #[test]
    fn no_max_tokens_is_written_when_nothing_sets_one() {
        let mut body = json!({});
        resolve_sampling(
            &mut body,
            &ModelContext::passthrough(),
            &SamplingLayers::default(),
        );
        assert!(body.as_object().unwrap().get("max_tokens").is_none());
    }

    #[test]
    fn a_non_object_body_is_left_alone() {
        let mut body = json!([1, 2, 3]);
        resolve_sampling(
            &mut body,
            &ModelContext::passthrough(),
            &SamplingLayers::default(),
        );
        assert_eq!(body, json!([1, 2, 3]));
    }
}
