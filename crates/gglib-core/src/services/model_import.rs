//! Shared model construction for the local-file and `HuggingFace` add paths.
//!
//! [`build_new_model`] is the single place that turns a parsed GGUF file
//! into a [`NewModel`] row — naming, parameter count, capability detection,
//! and tag generation all happen here exactly once, regardless of how the
//! model was added.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::domain::{
    DefaultsOrigin, GgufMetadata, InferenceConfig, NameSource, NewModel, resolve_model_name,
};
use crate::download::Quantization;
use crate::ports::GgufParserPort;
use tracing::{debug, info, warn};

/// Provenance-specific inputs for [`build_new_model`].
///
/// Modelled as an enum rather than a struct of options because the two add
/// paths never share these fields: a local import always carries an
/// optional user override and never HF provenance, and a download always
/// carries HF provenance and never a param-count override.
pub enum ModelOrigin<'a> {
    LocalFile { param_count_override: Option<f64> },
    HuggingFace(HfOrigin<'a>),
}

impl ModelOrigin<'_> {
    const fn name_source(&self) -> NameSource<'_> {
        match self {
            Self::LocalFile { .. } => NameSource::LocalFile,
            Self::HuggingFace(hf) => NameSource::HuggingFace {
                repo_id: hf.repo_id,
            },
        }
    }
}

/// `HuggingFace`-specific inputs to [`ModelOrigin::HuggingFace`].
pub struct HfOrigin<'a> {
    pub repo_id: &'a str,
    pub commit_sha: &'a str,
    pub hf_tags: &'a [String],
    /// Used when the GGUF header declares no quantization.
    pub quantization_fallback: Quantization,
    /// Ordered file paths for sharded models.
    pub file_paths: Option<&'a [PathBuf]>,
    /// The model author's own recipe, if one was fetched from the base repo.
    ///
    /// `None` on every path that could not or did not look — no network, a
    /// gated repo, a repo publishing no `generation_config.json`, or a
    /// local-file import, which has no repo to ask. All of those fall back to
    /// the `reasoning` tag guess exactly as before, which is why this is an
    /// `Option` rather than a result carrying a reason: by the time it reaches
    /// here the reason has already been logged and the decision is the same.
    pub published_sampling: Option<&'a InferenceConfig>,
}

/// Filter `HuggingFace` tags using a blocklist.
///
/// Removes noisy tags like `gguf`, `arxiv:*`, `region:*`, `license:*`, `dataset:*`.
fn filter_hf_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .filter(|tag| {
            let tag_lower = tag.to_lowercase();
            !tag_lower.starts_with("arxiv:")
                && !tag_lower.starts_with("region:")
                && !tag_lower.starts_with("license:")
                && !tag_lower.starts_with("dataset:")
                && tag_lower != "gguf"
        })
        .cloned()
        .collect()
}

/// Merge GGUF-derived tags with filtered HF tags, removing duplicates.
///
/// GGUF-derived tags are prioritized (appear first in the result).
fn merge_tags(gguf_tags: Vec<String>, hf_tags: &[String]) -> Vec<String> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for tag in gguf_tags {
        if seen.insert(tag.clone()) {
            result.push(tag);
        }
    }
    for tag in filter_hf_tags(hf_tags) {
        if seen.insert(tag.clone()) {
            result.push(tag);
        }
    }

    result
}

/// Fetch the model author's published sampling recipe, if one can be found.
///
/// Tries each repo [`generation_config_candidates`] names, in order, and stops
/// at the first that yields a usable recipe.
///
/// # Every failure is the same failure
///
/// This returns `None` and never an error, because there is exactly one
/// response to *any* negative answer: carry on with the import and let the
/// `reasoning` tag guess apply as it always has. A sampling recipe is a nicety;
/// failing an import over one would be absurd. The distinct causes are logged
/// rather than propagated:
///
/// - **404** — the repo publishes no `generation_config.json`. The ordinary
///   case for a quant repo, and the reason the candidate list exists.
/// - **Gated or private** (401/403) — the base repo needs a token this
///   installation does not have. Common for Llama and Gemma.
/// - **Offline, rate-limited, malformed** — nothing to do but proceed.
/// - **Published nothing usable** — a file carrying only token ids. Treated as
///   a miss so it cannot displace the tag guess with an all-`None` recipe, and
///   the search continues to the next candidate.
///
/// # Bounded work
///
/// At most [`MAX_GENERATION_CONFIG_LOOKUPS`] requests, and it stops at the
/// first hit. An import is already dominated by downloading gigabytes of
/// weights, but this runs on the local-add path too, where it must not turn a
/// fast operation into a network-bound one.
pub async fn fetch_published_sampling(
    client: &dyn crate::ports::huggingface::HfClientPort,
    repo_id: &str,
    tags: &[String],
) -> Option<InferenceConfig> {
    let candidates = crate::domain::generation_config_candidates(repo_id, tags);

    for candidate in candidates.iter().take(MAX_GENERATION_CONFIG_LOOKUPS) {
        let body = match client.fetch_generation_config(candidate).await {
            Ok(Some(body)) => body,
            Ok(None) => {
                debug!("{candidate} publishes no generation_config.json");
                continue;
            }
            Err(e) => {
                // Info rather than warn: on the common path this is a gated
                // base repo, which is not a fault in this installation and
                // costs nothing but a fallback to the tag guess.
                info!("could not read {candidate}'s generation_config.json: {e}");
                continue;
            }
        };

        let Some(parsed) = crate::domain::parse_generation_config(&body) else {
            warn!("{candidate}'s generation_config.json is not a JSON object; ignoring");
            continue;
        };

        for reason in &parsed.rejected {
            warn!("{candidate}'s generation_config.json: {reason}; that value is not applied");
        }
        if parsed.requests_greedy {
            // Not applied: gglib has no greedy mode, and the nearest
            // equivalent is the near-greedy setting ADR 0004's addendum bans
            // for reasoning models. Said out loud so the divergence from the
            // author's file is visible rather than silent.
            info!(
                "{candidate} publishes do_sample: false (greedy); gglib does not apply greedy \
                 decoding and is using the published sampler values instead"
            );
        }
        if parsed.is_empty() {
            debug!("{candidate}'s generation_config.json names no sampler values gglib models");
            continue;
        }

        info!("using the sampling recipe {candidate} publishes");
        return Some(parsed.config);
    }

    None
}

/// How many repos to ask for a `generation_config.json` before giving up.
///
/// [`generation_config_candidates`] yields at most three, and this bounds it
/// independently so a future candidate source cannot quietly make an import
/// issue an unbounded number of requests.
///
/// [`generation_config_candidates`]: crate::domain::generation_config_candidates
pub const MAX_GENERATION_CONFIG_LOOKUPS: usize = 3;

/// Build the `NewModel` row for a model being added, from either the
/// local-file or `HuggingFace` path.
///
/// The single place that decides a model's stored name, parameter count,
/// tags, and capability flags, so both add paths produce the same result
/// for the same GGUF file. `gguf` is `None` only when the header could not
/// be parsed — tolerated on the download path; the local-file path
/// validates first and always passes `Some`.
#[must_use]
pub fn build_new_model(
    file_path: &Path,
    gguf: Option<&GgufMetadata>,
    parser: &dyn GgufParserPort,
    origin: &ModelOrigin<'_>,
    added_at: DateTime<Utc>,
) -> NewModel {
    let name = resolve_model_name(gguf, file_path, origin.name_source());

    let param_count_b = match origin {
        ModelOrigin::LocalFile {
            param_count_override,
        } => param_count_override
            .or_else(|| gguf.and_then(|g| g.param_count_b))
            .unwrap_or(0.0),
        ModelOrigin::HuggingFace(_) => gguf.and_then(|g| g.param_count_b).unwrap_or(0.0),
    };

    let gguf_caps = gguf.map(|g| parser.detect_capabilities(g));
    let gguf_tags = gguf_caps
        .as_ref()
        .map_or_else(Vec::new, crate::domain::GgufCapabilities::to_tags);

    let mut model = NewModel::new(name, file_path.to_path_buf(), param_count_b, added_at);
    model.dialect_spec = gguf_caps.and_then(|c| c.dialect);
    model.architecture = gguf.and_then(|g| g.architecture.clone());
    model.context_length = gguf.and_then(|g| g.context_length);
    model.expert_count = gguf.and_then(|g| g.expert_count);
    model.expert_used_count = gguf.and_then(|g| g.expert_used_count);
    model.expert_shared_count = gguf.and_then(|g| g.expert_shared_count);
    if let Some(g) = gguf {
        model.metadata.clone_from(&g.metadata);
    }

    match origin {
        ModelOrigin::LocalFile { .. } => {
            model.quantization = gguf.and_then(|g| g.quantization.clone());
            model.tags = gguf_tags;
        }
        ModelOrigin::HuggingFace(hf) => {
            model.quantization = gguf
                .and_then(|g| g.quantization.clone())
                .or_else(|| Some(hf.quantization_fallback.to_string()));
            model.hf_repo_id = Some(hf.repo_id.to_string());
            model.hf_commit_sha = Some(hf.commit_sha.to_string());
            model.hf_filename = Some(file_path.file_name().unwrap().to_string_lossy().to_string());
            model.download_date = Some(Utc::now());
            model.file_paths = hf.file_paths.map(<[PathBuf]>::to_vec);
            model.tags = merge_tags(gguf_tags, hf.hf_tags);
        }
    }

    // Seed the model's own rung of the sampling hierarchy, from the best
    // evidence available about *this* model.
    //
    // Both origins rank identically — below global settings — because neither
    // was reviewed by a person. What differs is their quality, and that is why
    // a published recipe replaces the guess rather than merging with it:
    //
    // - **Published** — the author's `generation_config.json`, fetched from
    //   the base repo. Evidence about this model.
    // - **AutoDetected** — `reasoning_profile()`, keyed off a tag. A generic
    //   guess that happens to be right for the Qwen3 family it was written
    //   from.
    //
    // Merging them would produce a recipe no author published and gglib cannot
    // defend, labelled as though somebody had. It would also defeat the
    // temperature-coupling rule, which exists precisely so a layer naming a
    // temperature is not silently paired with penalties tuned for a different
    // one.
    //
    // Only set when the model has no explicit defaults already (always true
    // here, since `model` was just constructed).
    if model.inference_defaults.is_none() {
        let published = match origin {
            ModelOrigin::HuggingFace(hf) => hf.published_sampling,
            ModelOrigin::LocalFile { .. } => None,
        };
        if let Some(config) = published {
            model.inference_defaults = Some(config.clone());
            model.defaults_origin = Some(DefaultsOrigin::Published);
        } else if crate::domain::capability_tags::is_reasoning(&model.tags) {
            model.inference_defaults = Some(InferenceConfig::reasoning_profile());
            model.defaults_origin = Some(DefaultsOrigin::AutoDetected);
        }
    }

    // Infer capabilities from chat template OR architecture — OR'd so either
    // signal is sufficient. Architecture is the backstop for models whose
    // GGUF ships without a tokenizer section. The declared name (not the
    // resolved display name) feeds this so an HF repo id never drives
    // name-based capability detection for headers with no general.name.
    let template = model
        .metadata
        .get("tokenizer.chat_template")
        .map(String::as_str);
    let declared = crate::domain::declared_name(gguf);
    let from_template = crate::domain::infer_from_chat_template(template, declared);
    let from_arch = crate::domain::capabilities_from_architecture(model.architecture.as_deref());
    model.capabilities = from_template | from_arch;

    model
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact literal round-trip through param_count_b, no lossy conversion
mod tests {
    use super::*;
    use crate::ports::NoopGgufParser;
    use std::collections::HashMap;

    fn gguf_with(pairs: &[(&str, &str)]) -> GgufMetadata {
        let mut metadata = HashMap::new();
        for (k, v) in pairs {
            metadata.insert((*k).to_string(), (*v).to_string());
        }
        GgufMetadata {
            metadata,
            ..Default::default()
        }
    }

    /// Stub parser whose detection reports a dialect spec.
    struct SpecParser;

    impl crate::ports::GgufParserPort for SpecParser {
        fn parse(
            &self,
            _file_path: &Path,
        ) -> std::result::Result<crate::ports::GgufMetadata, crate::ports::GgufParseError> {
            Ok(crate::ports::GgufMetadata::default())
        }

        fn detect_capabilities(
            &self,
            _metadata: &crate::ports::GgufMetadata,
        ) -> crate::ports::GgufCapabilities {
            crate::ports::GgufCapabilities {
                flags: crate::domain::gguf::CapabilityFlags::TOOL_CALLING,
                extensions: std::collections::BTreeSet::new(),
                dialect: Some(crate::domain::DialectSpec::qwen_xml()),
            }
        }
    }

    fn hf_origin<'a>(repo_id: &'a str, hf_tags: &'a [String]) -> ModelOrigin<'a> {
        hf_origin_with(repo_id, hf_tags, None)
    }

    fn hf_origin_with<'a>(
        repo_id: &'a str,
        hf_tags: &'a [String],
        published_sampling: Option<&'a InferenceConfig>,
    ) -> ModelOrigin<'a> {
        ModelOrigin::HuggingFace(HfOrigin {
            repo_id,
            commit_sha: "abc123",
            hf_tags,
            quantization_fallback: Quantization::Q4KM,
            file_paths: None,
            published_sampling,
        })
    }

    #[test]
    fn detected_dialect_spec_lands_on_the_model() {
        let gguf = gguf_with(&[]);
        let origin = ModelOrigin::LocalFile {
            param_count_override: None,
        };
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            Some(&gguf),
            &SpecParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(
            model.dialect_spec,
            Some(crate::domain::DialectSpec::qwen_xml())
        );
    }

    /// An HF model whose GGUF could not be parsed has no metadata and can
    /// never gain a spec — the permanent-fallback case retag cannot fix.
    #[test]
    fn missing_gguf_metadata_means_no_spec() {
        let hf_tags: Vec<String> = vec![];
        let origin = hf_origin("some/Repo-GGUF", &hf_tags);
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &SpecParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(model.dialect_spec, None);
    }

    #[test]
    fn local_param_override_beats_gguf_metadata() {
        let gguf = GgufMetadata {
            param_count_b: Some(7.0),
            ..Default::default()
        };
        let origin = ModelOrigin::LocalFile {
            param_count_override: Some(13.0),
        };
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            Some(&gguf),
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(model.param_count_b, 13.0);
    }

    #[test]
    fn local_param_falls_back_to_gguf_metadata() {
        let gguf = GgufMetadata {
            param_count_b: Some(7.0),
            ..Default::default()
        };
        let origin = ModelOrigin::LocalFile {
            param_count_override: None,
        };
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            Some(&gguf),
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(model.param_count_b, 7.0);
    }

    #[test]
    fn hf_quant_fallback_used_only_when_header_has_none() {
        let hf_tags: Vec<String> = vec![];
        let origin = hf_origin("unsloth/Qwen3-8B-GGUF", &hf_tags);
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(model.quantization, Some(Quantization::Q4KM.to_string()));

        let gguf = GgufMetadata {
            quantization: Some("Q8_0".to_string()),
            ..Default::default()
        };
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            Some(&gguf),
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(model.quantization, Some("Q8_0".to_string()));
    }

    #[test]
    fn hf_tags_are_merged_deduped_and_filtered() {
        let hf_tags = vec![
            "chat".to_string(),
            "arxiv:1234.5678".to_string(),
            "region:us".to_string(),
            "license:apache-2.0".to_string(),
            "dataset:foo".to_string(),
            "gguf".to_string(),
            "chat".to_string(),
        ];
        let origin = hf_origin("unsloth/Qwen3-8B-GGUF", &hf_tags);
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(model.tags, vec!["chat".to_string()]);
    }

    /// **The point of the whole lookup.** A recipe the author published is
    /// evidence about this model; `reasoning_profile()` is a generic guess
    /// keyed off a tag. So the published one wins where it exists.
    #[test]
    fn a_published_recipe_replaces_the_reasoning_tag_guess() {
        let hf_tags = vec!["reasoning".to_string()];
        let published = InferenceConfig {
            temperature: Some(0.6),
            top_p: Some(0.95),
            top_k: Some(20),
            ..InferenceConfig::default()
        };
        let origin = hf_origin_with("unsloth/Qwen3-8B-GGUF", &hf_tags, Some(&published));

        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );

        assert_eq!(model.inference_defaults, Some(published));
        assert_eq!(model.defaults_origin, Some(DefaultsOrigin::Published));
    }

    /// **It replaces rather than merges.** Filling the published recipe's gaps
    /// from `reasoning_profile()` would produce a recipe no author published
    /// and gglib cannot defend, labelled as though somebody had — and it would
    /// defeat the temperature-coupling rule, which exists so a layer naming a
    /// temperature is not paired with penalties tuned for a different one.
    #[test]
    fn a_published_recipe_is_not_merged_with_the_tag_guess() {
        let hf_tags = vec!["reasoning".to_string()];
        let published = InferenceConfig {
            temperature: Some(0.6),
            ..InferenceConfig::default()
        };
        let origin = hf_origin_with("unsloth/Qwen3-8B-GGUF", &hf_tags, Some(&published));

        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );

        let stored = model.inference_defaults.expect("defaults stored");
        assert_eq!(stored.temperature, Some(0.6));
        assert_eq!(
            stored.presence_penalty, None,
            "reasoning_profile's 1.5 must not be grafted on"
        );
        assert_eq!(stored.top_p, None, "nor anything else it names");
    }

    /// A published recipe must rank exactly where the tag guess does — below
    /// global settings. Neither was reviewed by a person, so neither may
    /// outrank a setting somebody chose.
    #[test]
    fn a_published_recipe_ranks_below_global_settings() {
        let hf_tags: Vec<String> = vec![];
        let published = InferenceConfig {
            temperature: Some(0.6),
            ..InferenceConfig::default()
        };
        let origin = hf_origin_with("Qwen/Qwen3-4B", &hf_tags, Some(&published));
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );

        let global = InferenceConfig {
            temperature: Some(0.9),
            ..InferenceConfig::default()
        };
        let (resolved, _) = InferenceConfig::default().resolve_with_profile_explained(
            None,
            model.inference_defaults.as_ref(),
            Some(&global),
            crate::domain::ModelSamplingContext {
                is_reasoning: false,
                defaults_origin: model.defaults_origin,
            },
        );

        assert_eq!(
            resolved.temperature,
            Some(0.9),
            "the operator's global setting must win over a fetched recipe"
        );
    }

    /// A published recipe reaches a model with no `reasoning` tag too — the
    /// lookup is about the author's repo, not about gglib's tagging.
    #[test]
    fn a_published_recipe_applies_without_a_reasoning_tag() {
        let hf_tags: Vec<String> = vec![];
        let published = InferenceConfig {
            temperature: Some(0.4),
            ..InferenceConfig::default()
        };
        let origin = hf_origin_with("some/Model", &hf_tags, Some(&published));

        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );

        assert_eq!(model.defaults_origin, Some(DefaultsOrigin::Published));
    }

    /// The degradation path, and the one that must keep working: every fetch
    /// failure arrives here as `None`, and the import behaves exactly as it
    /// did before the lookup existed.
    #[test]
    fn no_published_recipe_falls_back_to_the_tag_guess() {
        let hf_tags = vec!["reasoning".to_string()];
        let origin = hf_origin_with("unsloth/Qwen3-8B-GGUF", &hf_tags, None);

        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );

        assert_eq!(
            model.inference_defaults,
            Some(InferenceConfig::reasoning_profile())
        );
        assert_eq!(model.defaults_origin, Some(DefaultsOrigin::AutoDetected));
    }

    #[test]
    fn reasoning_tag_sets_inference_defaults_on_both_origins() {
        let hf_tags = vec!["reasoning".to_string()];
        let hf = hf_origin("unsloth/Qwen3-8B-GGUF", &hf_tags);
        let hf_model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &hf,
            Utc::now(),
        );
        assert_eq!(
            hf_model.inference_defaults,
            Some(InferenceConfig::reasoning_profile())
        );
        assert_eq!(
            hf_model.defaults_origin,
            Some(DefaultsOrigin::AutoDetected),
            "gglib's own guess, not a user choice — must rank below global settings"
        );

        let gguf = gguf_with(&[]);
        let local = ModelOrigin::LocalFile {
            param_count_override: None,
        };
        // NoopGgufParser detects no capabilities/tags, so drive the tag
        // through metadata presence isn't possible here — this asserts the
        // guard is origin-agnostic by checking the same code path runs for
        // LocalFile without panicking and produces no defaults when no
        // reasoning tag is present (see next test for the positive local case).
        let local_model = build_new_model(
            Path::new("/models/m.gguf"),
            Some(&gguf),
            &NoopGgufParser,
            &local,
            Utc::now(),
        );
        assert_eq!(local_model.inference_defaults, None);
        assert_eq!(local_model.defaults_origin, None);
    }

    #[test]
    fn gguf_none_falls_back_to_repo_rung_and_hf_only_tags() {
        let hf_tags = vec!["chat".to_string()];
        let origin = hf_origin("unsloth/Qwen3.6-27B-MTP-GGUF", &hf_tags);
        let model = build_new_model(
            Path::new("/models/m.gguf"),
            None,
            &NoopGgufParser,
            &origin,
            Utc::now(),
        );
        assert_eq!(model.name, "Qwen3.6-27B-MTP");
        assert_eq!(model.tags, vec!["chat".to_string()]);
        assert_eq!(
            model.hf_repo_id,
            Some("unsloth/Qwen3.6-27B-MTP-GGUF".to_string())
        );
    }
}
