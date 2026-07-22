//! Stage 4–5: resolving what the model is asked to sample with.
//!
//! Unlike [`super::messages`], nothing here reads `messages` — these transforms
//! only ever touch top-level keys.

use serde_json::Value;
use tracing::debug;

use super::ModelContext;
use crate::domain::InferenceConfig;

/// The sampling layers that sit *below* the client's own request parameters.
///
/// Grouped because they are only ever used together, at the single point where
/// [`InferenceConfig::resolve_with_profile`] runs.
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
}

/// Which layer supplied each resolved sampling value, as `field=layer` pairs.
///
/// Nothing else records this. Without it the effective sampling config is
/// invisible at every log level, and answering "where did this
/// `presence_penalty` come from?" means probing llama-server's live `/slots`
/// mid-generation — which is how the leak behind #621 had to be found.
///
/// Mirrors the ladder in [`InferenceConfig::resolve_with_profile`], including
/// the temperature coupling: once a layer declares a `temperature`, layers
/// below it can no longer supply the parameters tuned against it, so those
/// report `hardcoded` even though a lower layer names a value.
fn describe_provenance(
    client: &InferenceConfig,
    model: Option<&InferenceConfig>,
    layers: &SamplingLayers,
) -> String {
    let ordered: [(&str, Option<&InferenceConfig>); 5] = [
        ("cli", layers.cli_override.as_ref()),
        ("client", Some(client)),
        ("profile", layers.profile.as_ref()),
        ("model", model),
        ("global", layers.global.as_ref()),
    ];

    // The highest layer naming a temperature; layers below it cannot supply
    // temperature-tuned parameters. `None` means nothing claimed it, so every
    // layer stays eligible.
    let claim = ordered
        .iter()
        .position(|(_, c)| c.is_some_and(|c| c.temperature.is_some()))
        .unwrap_or(ordered.len());

    let source = |declares: &dyn Fn(&InferenceConfig) -> bool, limit: usize| -> &'static str {
        ordered
            .iter()
            .take(limit)
            .find(|(_, c)| c.is_some_and(|c| declares(c)))
            .map_or("hardcoded", |(name, _)| name)
    };

    let all = ordered.len();
    // Tuned parameters are eligible only up to and including the claiming layer.
    let tuned = claim.saturating_add(1).min(all);
    format!(
        "temperature={} top_p={} top_k={} max_tokens={} presence_penalty={} repeat_penalty={} min_p={}",
        source(&|c| c.temperature.is_some(), all),
        source(&|c| c.top_p.is_some(), all),
        source(&|c| c.top_k.is_some(), all),
        source(&|c| c.max_tokens.is_some(), all),
        source(&|c| c.presence_penalty.is_some(), tuned),
        source(&|c| c.repeat_penalty.is_some(), tuned),
        source(&|c| c.min_p.is_some(), tuned),
    )
}

/// Resolve the sampling hierarchy into `body`, then pin `cache_prompt`.
///
/// # Force-insert, not `or_insert`
///
/// The client's own parameters are extracted from `body` first, merged
/// **beneath** profile / model / global by
/// [`InferenceConfig::resolve_with_profile`], and the fully-resolved result is
/// then written back over the top. Client parameters still win — they win by
/// being the highest-priority layer *inside* the merge, not by surviving an
/// `or_insert`. Rewriting this as `or_insert` looks equivalent and silently
/// breaks the hierarchy: every layer below the client would stop applying to
/// any key the client happened to send.
///
/// A body that is not a JSON object is left alone.
pub fn resolve_sampling(body: &mut Value, ctx: &ModelContext, layers: &SamplingLayers) {
    let client_params = InferenceConfig::from_openai_json(body);
    // Operator flags sit above the client, everything else below it.
    let top = match layers.cli_override.as_ref() {
        Some(o) => o.clone().stacked_over(&client_params),
        None => client_params.clone(),
    };
    let resolved = top.resolve_with_profile(
        layers.profile.as_ref(),
        ctx.inference_defaults.as_ref(),
        layers.global.as_ref(),
    );

    if tracing::enabled!(tracing::Level::DEBUG) {
        debug!(
            temperature = ?resolved.temperature,
            top_p = ?resolved.top_p,
            top_k = ?resolved.top_k,
            max_tokens = ?resolved.max_tokens,
            presence_penalty = ?resolved.presence_penalty,
            repeat_penalty = ?resolved.repeat_penalty,
            min_p = ?resolved.min_p,
            from = %describe_provenance(&client_params, ctx.inference_defaults.as_ref(), layers),
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
            },
        );

        assert_param(&body, "temperature", 0.2);
        assert_param(&body, "top_p", 0.87);
        assert_param(&body, "top_k", 20.0);
    }

    // ── Provenance ────────────────────────────────────────────────────────

    /// The `:coding` shape. The provenance string must say the penalty came
    /// from the neutral floor, not from the model — otherwise the log would
    /// assert exactly the leak the merge now prevents.
    #[test]
    fn provenance_reports_coupling_suppressed_layers_as_hardcoded() {
        let model = InferenceConfig {
            temperature: Some(1.0),
            presence_penalty: Some(1.5),
            top_k: Some(20),
            ..Default::default()
        };
        let got = describe_provenance(
            &InferenceConfig::default(),
            Some(&model),
            &SamplingLayers {
                profile: Some(temp(0.2)),
                ..Default::default()
            },
        );

        assert!(got.contains("temperature=profile"), "{got}");
        assert!(got.contains("presence_penalty=hardcoded"), "{got}");
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
        let got = describe_provenance(
            &InferenceConfig::default(),
            Some(&model),
            &SamplingLayers::default(),
        );

        assert!(got.contains("temperature=model"), "{got}");
        assert!(got.contains("presence_penalty=model"), "{got}");
    }

    /// Operator flags are reported as their own layer, above the client.
    #[test]
    fn provenance_names_the_cli_layer() {
        let got = describe_provenance(
            &temp(0.9),
            None,
            &SamplingLayers {
                cli_override: Some(temp(0.3)),
                ..Default::default()
            },
        );

        assert!(got.contains("temperature=cli"), "{got}");
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
            },
        );

        assert_param(&body, "temperature", 0.2);
        assert_param(&body, "presence_penalty", 0.0);
    }

    /// The force-insert. An `or_insert` implementation passes every test above
    /// and fails this one.
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
            &SamplingLayers::default(),
        );

        assert_param(&body, "temperature", 0.11);
        assert_param(&body, "top_p", 0.42);
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
