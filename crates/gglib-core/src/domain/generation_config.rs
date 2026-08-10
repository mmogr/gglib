//! The sampling recipe a model author publishes in `generation_config.json`.
//!
//! # Why gglib goes looking for this
//!
//! [`crate::domain::model_sampling`] reads `general.sampling.*` out of a GGUF,
//! which llama.cpp applies directly. That is the ideal source and it is almost
//! never present: the keys landed upstream in PR #17120 (2025-11-25) and most
//! converters predate them or simply drop what they do not recognise. The
//! author's numbers exist — they are in `generation_config.json` in the base
//! repo, which is what every `transformers` user gets by default — they just do
//! not survive the trip into a quantised GGUF.
//!
//! So gglib fetches them at import instead, and ranks them where an unreviewed
//! recipe belongs. Same argument [ADR 0004]'s follow-up makes:
//!
//! > gglib currently writes its own `reasoning_profile()` recipe for
//! > `reasoning`-tagged models; a model author's published recommendation is
//! > better evidence than gglib's guess.
//!
//! # This is not `ModelSamplingDefaults`, and must not become it
//!
//! The obvious place to put these values is [`ModelSamplingDefaults`], and it
//! is the wrong one. That type means *"what this GGUF declares, which
//! llama.cpp has already applied to `default_generation_settings`"*, and
//! `gglib_proxy::props` reads it to decide whether a `/props` value is
//! attributable to the model rather than to the build.
//!
//! A value fetched from `HuggingFace` has been applied by nobody. Writing it
//! there would make the baseline check report `ModelSupplied` for a number
//! llama-server never saw — an instrument reporting an attribution that cannot
//! be wrong because it was invented, which is [ADR 0004] finding 1's trap
//! wearing a new hat. These stay in the *inference hierarchy*, where a value
//! only takes effect because gglib sends it.
//!
//! [`ModelSamplingDefaults`]: crate::domain::ModelSamplingDefaults
//! [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md

use serde_json::Value;

use super::InferenceConfig;

/// Where to look for a `generation_config.json`, best candidate first.
///
/// The file lives in the *base* repo — `Qwen/Qwen3-4B` — while the GGUF a
/// person downloads almost always comes from a quant repo such as
/// `unsloth/Qwen3-4B-GGUF`, which carries the weights and little else. Asking
/// only the repo gglib downloaded from would find nothing on the overwhelming
/// majority of imports.
///
/// Three sources, in descending order of how much they are actually knowledge:
///
/// 1. **`base_model:` tags.** `HuggingFace` generates these on a quant repo
///    from its model card, in the forms `base_model:Qwen/Qwen3-4B` and
///    `base_model:quantized:Qwen/Qwen3-4B`. This is the publisher stating the
///    relationship, so it goes first.
/// 2. **The download repo itself.** Some publishers do ship the file beside
///    the GGUFs, and a repo that is *not* a quant repo is its own base.
/// 3. **The name with a quant suffix stripped**, under the same owner. A
///    guess, and last for that reason — `unsloth/Qwen3-4B-GGUF` implies
///    `unsloth/Qwen3-4B`, which frequently does not exist. Harmless because a
///    miss is a 404 the caller already has to handle.
///
/// Deduplicated, order-preserving, so a repo that is its own base is not asked
/// twice.
#[must_use]
pub fn generation_config_candidates(repo_id: &str, tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |candidate: String| {
        if !candidate.is_empty() && !out.contains(&candidate) {
            out.push(candidate);
        }
    };

    for tag in tags {
        if let Some(rest) = tag.strip_prefix("base_model:") {
            // `base_model:quantized:Owner/Name`, `base_model:finetune:...` and
            // the bare `base_model:Owner/Name` all end with the repo id, so
            // take the last two path segments rather than enumerating the
            // relationship words — which HuggingFace adds to over time.
            let segments: Vec<&str> = rest.split('/').collect();
            if segments.len() >= 2 {
                let owner = segments[segments.len() - 2]
                    .rsplit(':')
                    .next()
                    .unwrap_or_default();
                push(format!("{owner}/{}", segments[segments.len() - 1]));
            }
        }
    }

    push(repo_id.to_owned());

    if let Some((owner, name)) = repo_id.split_once('/') {
        let trimmed = strip_quant_suffix(name);
        if trimmed != name {
            push(format!("{owner}/{trimmed}"));
        }
    }

    out
}

/// Strip a trailing quantisation marker from a repo name.
fn strip_quant_suffix(name: &str) -> &str {
    for suffix in ["-GGUF", "-gguf", ".GGUF", ".gguf", "-GGML", "-ggml"] {
        if let Some(base) = name.strip_suffix(suffix) {
            return base;
        }
    }
    name
}

/// The `transformers` spelling of each field gglib models, paired with its own.
///
/// Only the fields where the two mean the same thing. The omissions below are
/// the interesting part of this module.
const FIELD_MAP: [(&str, &str); 6] = [
    ("temperature", "temperature"),
    ("top_p", "top_p"),
    ("top_k", "top_k"),
    // `transformers` calls it `repetition_penalty`; llama.cpp and gglib call
    // it `repeat_penalty`. Same flat multiplicative penalty, same semantics.
    ("repetition_penalty", "repeat_penalty"),
    ("min_p", "min_p"),
    // Rare in practice, but unambiguous where present.
    ("presence_penalty", "presence_penalty"),
];

/// Accepted range for each field, matching `docs/sampling.md`.
///
/// A published value outside its range is dropped rather than clamped. Clamping
/// would invent a number the author did not choose and attribute it to them;
/// dropping falls through to the next rung, which is a source gglib can name.
fn in_range(field: &str, value: f64) -> bool {
    match field {
        // `temperature` and `presence_penalty` share a range by coincidence
        // rather than by kind; they are one arm because clippy objects to two
        // identical ones, not because the bound has a common origin.
        "temperature" | "presence_penalty" => (0.0..=2.0).contains(&value),
        "top_p" | "min_p" => (0.0..=1.0).contains(&value),
        // llama.cpp treats 0 as "disabled"; negatives are meaningless.
        "top_k" => value >= 0.0 && value <= f64::from(i32::MAX),
        "repeat_penalty" => value > 0.0,
        _ => false,
    }
}

/// What one `generation_config.json` yielded.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedGenerationConfig {
    /// The sampling values gglib could use, ready to occupy a ladder rung.
    pub config: InferenceConfig,
    /// Fields that were present but unusable — out of range, or not a number.
    ///
    /// Kept rather than dropped silently so an import can say *why* an author's
    /// published value is not being honoured. A number gglib ignored without
    /// saying so is indistinguishable from one it never saw.
    pub rejected: Vec<String>,
    /// Whether the file asked for greedy decoding via `do_sample: false`.
    ///
    /// Reported rather than acted on. gglib has no greedy mode, and the nearest
    /// equivalent — forcing `temperature: 0` — is exactly the near-greedy
    /// setting [ADR 0004]'s addendum bans for reasoning models. So this is
    /// surfaced for a person to decide about, never applied.
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    pub requests_greedy: bool,
}

impl PublishedGenerationConfig {
    /// Whether the file yielded any usable sampling value at all.
    ///
    /// A `generation_config.json` carrying only token ids is the common case,
    /// and it must not be stored as a recipe — an all-`None` config on the
    /// ladder is indistinguishable from no rung, but it would displace the
    /// `reasoning` recipe that *does* carry values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.config == InferenceConfig::default()
    }
}

/// Read a model author's `generation_config.json`.
///
/// Returns `None` only when the body is not a JSON object. Anything else — an
/// empty file, one carrying nothing but token ids, values of the wrong type —
/// yields a [`PublishedGenerationConfig`] that reports what it found, because
/// "the author published nothing usable" and "gglib could not read the file"
/// are different answers and only the second is a fault.
///
/// # What is deliberately not read
///
/// - **`max_new_tokens` / `max_length`.** `max_tokens` is unset by design in
///   gglib (see [`InferenceConfig`]), so that a client's own limit is the only
///   thing that bounds a response. Importing an author's default would quietly
///   cap every request that named none, which is a much larger decision than
///   this module is making.
/// - **`do_sample: false`.** Reported via
///   [`PublishedGenerationConfig::requests_greedy`], never applied.
/// - **Everything else in the file** — `bos_token_id`, `eos_token_id`,
///   `pad_token_id`, `transformers_version` and friends. Naming only the
///   fields gglib models keeps this from becoming an obligation to track the
///   whole `transformers` generation schema, the same rule `SlotParams` states
///   about `/props`'s 42 fields.
#[must_use]
pub fn parse_generation_config(body: &str) -> Option<PublishedGenerationConfig> {
    let json: Value = serde_json::from_str(body).ok()?;
    let object = json.as_object()?;

    let mut config = InferenceConfig::default();
    let mut rejected = Vec::new();

    for (their_name, our_name) in FIELD_MAP {
        let Some(raw) = object.get(their_name) else {
            continue;
        };
        // An explicit `null` is how `transformers` spells "no opinion". It is
        // an absence, not a malformed value, so it is not a rejection.
        if raw.is_null() {
            continue;
        }
        let Some(value) = raw.as_f64() else {
            rejected.push(format!("{their_name} is not a number"));
            continue;
        };
        if !value.is_finite() || !in_range(our_name, value) {
            rejected.push(format!("{their_name} = {value} is out of range"));
            continue;
        }
        apply(&mut config, our_name, value);
    }

    Some(PublishedGenerationConfig {
        config,
        rejected,
        requests_greedy: object.get("do_sample") == Some(&Value::Bool(false)),
    })
}

/// Write one validated value onto the config by gglib's field name.
#[allow(clippy::cast_possible_truncation)]
fn apply(config: &mut InferenceConfig, field: &str, value: f64) {
    match field {
        "temperature" => config.temperature = Some(value as f32),
        "top_p" => config.top_p = Some(value as f32),
        // Already bounded to `0..=i32::MAX` by `in_range`.
        "top_k" => config.top_k = Some(value as i32),
        "repeat_penalty" => config.repeat_penalty = Some(value as f32),
        "min_p" => config.min_p = Some(value as f32),
        "presence_penalty" => config.presence_penalty = Some(value as f32),
        _ => unreachable!("apply is only called with a FIELD_MAP target"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Qwen3's published thinking-mode recipe, verbatim in shape.
    const QWEN3: &str = r#"{
        "bos_token_id": 151643,
        "do_sample": true,
        "eos_token_id": [151645, 151643],
        "pad_token_id": 151643,
        "repetition_penalty": 1.05,
        "temperature": 0.6,
        "top_k": 20,
        "top_p": 0.95,
        "transformers_version": "4.51.0"
    }"#;

    #[test]
    fn a_published_recipe_is_read_into_the_fields_gglib_models() {
        let parsed = parse_generation_config(QWEN3).expect("parses");

        assert_eq!(parsed.config.temperature, Some(0.6));
        assert_eq!(parsed.config.top_p, Some(0.95));
        assert_eq!(parsed.config.top_k, Some(20));
        assert!(parsed.rejected.is_empty(), "{:?}", parsed.rejected);
        assert!(!parsed.is_empty());
    }

    /// The one field whose name differs between `transformers` and gglib. A
    /// typo here would be silent — the value would simply never be read.
    #[test]
    fn repetition_penalty_maps_onto_repeat_penalty() {
        let parsed = parse_generation_config(QWEN3).expect("parses");
        assert_eq!(parsed.config.repeat_penalty, Some(1.05));
    }

    /// **The common case.** Most `generation_config.json` files carry nothing
    /// but token ids, and storing that as a recipe would displace the
    /// `reasoning` recipe that does carry values.
    #[test]
    fn a_file_with_only_token_ids_yields_an_empty_recipe() {
        let parsed = parse_generation_config(
            r#"{"bos_token_id": 1, "eos_token_id": 2, "transformers_version": "4.51.0"}"#,
        )
        .expect("parses");

        assert!(parsed.is_empty());
        assert!(parsed.rejected.is_empty(), "absence is not a rejection");
    }

    /// `max_new_tokens` must not become `max_tokens`: gglib leaves that unset
    /// by design so nothing but the client bounds a response.
    #[test]
    fn a_published_token_limit_is_not_imported() {
        let parsed = parse_generation_config(r#"{"max_new_tokens": 512, "max_length": 4096}"#)
            .expect("parses");

        assert_eq!(parsed.config.max_tokens, None);
        assert!(parsed.is_empty());
    }

    /// Greedy is reported, never applied — forcing `temperature: 0` is the
    /// near-greedy setting ADR 0004's addendum bans for reasoning models.
    #[test]
    fn a_request_for_greedy_decoding_is_reported_rather_than_applied() {
        let parsed =
            parse_generation_config(r#"{"do_sample": false, "temperature": 0.7}"#).expect("parses");

        assert!(parsed.requests_greedy);
        assert_eq!(
            parsed.config.temperature,
            Some(0.7),
            "the published temperature still stands on its own"
        );
    }

    #[test]
    fn do_sample_true_is_not_a_greedy_request() {
        let parsed = parse_generation_config(r#"{"do_sample": true}"#).expect("parses");
        assert!(!parsed.requests_greedy);
    }

    /// Out-of-range values are dropped rather than clamped: clamping invents a
    /// number the author did not choose and attributes it to them.
    #[test]
    fn an_out_of_range_value_is_dropped_and_reported() {
        let parsed =
            parse_generation_config(r#"{"temperature": 7.5, "top_p": 0.9}"#).expect("parses");

        assert_eq!(parsed.config.temperature, None, "not clamped to 2.0");
        assert_eq!(parsed.config.top_p, Some(0.9), "the good value still lands");
        assert_eq!(parsed.rejected.len(), 1);
        assert!(
            parsed.rejected[0].contains("temperature"),
            "{:?}",
            parsed.rejected
        );
    }

    #[test]
    fn a_non_numeric_value_is_reported_rather_than_silently_ignored() {
        let parsed = parse_generation_config(r#"{"temperature": "warm"}"#).expect("parses");

        assert_eq!(parsed.config.temperature, None);
        assert_eq!(parsed.rejected.len(), 1);
    }

    /// `null` is how `transformers` spells "no opinion". Reporting it as a
    /// rejection would put a warning on a file that is behaving normally.
    #[test]
    fn an_explicit_null_is_an_absence_not_a_rejection() {
        let parsed =
            parse_generation_config(r#"{"temperature": null, "top_k": null}"#).expect("parses");

        assert!(parsed.is_empty());
        assert!(parsed.rejected.is_empty());
    }

    /// `top_k: 0` means "disabled" in both `transformers` and llama.cpp, so it
    /// is a real value rather than an out-of-range one.
    #[test]
    fn top_k_zero_is_a_value_not_a_rejection() {
        let parsed = parse_generation_config(r#"{"top_k": 0}"#).expect("parses");

        assert_eq!(parsed.config.top_k, Some(0));
        assert!(parsed.rejected.is_empty());
    }

    /// A body that is not a JSON object is the one case gglib cannot read at
    /// all — distinct from a file that published nothing usable.
    #[test]
    fn an_unreadable_body_is_none_rather_than_an_empty_recipe() {
        assert!(parse_generation_config("not json").is_none());
        assert!(parse_generation_config("[1, 2, 3]").is_none());
        assert!(parse_generation_config("").is_none());

        assert!(
            parse_generation_config("{}").is_some(),
            "an empty object is a readable file that published nothing"
        );
    }

    // =========================================================================
    // Where to look
    // =========================================================================

    fn tags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    /// **The case this exists for.** The GGUF comes from a quant repo that
    /// carries weights and little else; the author's recipe is in the base
    /// repo, and the publisher already stated which one that is.
    #[test]
    fn a_base_model_tag_is_preferred_over_the_download_repo() {
        let candidates = generation_config_candidates(
            "unsloth/Qwen3-4B-GGUF",
            &tags(&["base_model:Qwen/Qwen3-4B", "text-generation"]),
        );

        assert_eq!(candidates[0], "Qwen/Qwen3-4B");
        assert!(candidates.contains(&"unsloth/Qwen3-4B-GGUF".to_string()));
    }

    /// `HuggingFace` inserts a relationship word, and adds new ones over time.
    /// Reading the last two segments survives words this code has never seen.
    #[test]
    fn a_relationship_qualified_base_model_tag_is_read() {
        for tag in [
            "base_model:quantized:Qwen/Qwen3-4B",
            "base_model:finetune:Qwen/Qwen3-4B",
            "base_model:some-future-word:Qwen/Qwen3-4B",
        ] {
            let candidates = generation_config_candidates("unsloth/Qwen3-4B-GGUF", &tags(&[tag]));
            assert_eq!(candidates[0], "Qwen/Qwen3-4B", "{tag}");
        }
    }

    /// The guess, and last on purpose: `unsloth/Qwen3-4B` frequently does not
    /// exist. Harmless, because a miss is a 404 the caller already handles.
    #[test]
    fn a_quant_suffix_is_stripped_as_a_last_resort() {
        let candidates = generation_config_candidates("unsloth/Qwen3-4B-GGUF", &[]);

        assert_eq!(candidates, ["unsloth/Qwen3-4B-GGUF", "unsloth/Qwen3-4B"]);
    }

    /// A repo that is its own base must not be asked twice.
    #[test]
    fn candidates_are_deduplicated_in_order() {
        let candidates =
            generation_config_candidates("Qwen/Qwen3-4B", &tags(&["base_model:Qwen/Qwen3-4B"]));

        assert_eq!(candidates, ["Qwen/Qwen3-4B"]);
    }

    #[test]
    fn a_repo_with_no_quant_suffix_yields_only_itself() {
        assert_eq!(
            generation_config_candidates("Qwen/Qwen3-4B", &[]),
            ["Qwen/Qwen3-4B"]
        );
    }

    /// A malformed tag must not produce a candidate that cannot be a repo id.
    #[test]
    fn a_base_model_tag_without_an_owner_is_skipped() {
        let candidates = generation_config_candidates("owner/Thing", &tags(&["base_model:Thing"]));
        assert_eq!(candidates, ["owner/Thing"]);
    }

    /// The list is what bounds the import's network work, so it must stay
    /// short whatever a repo tags itself with.
    #[test]
    fn the_candidate_list_stays_within_the_lookup_budget() {
        let noisy = tags(&[
            "base_model:Qwen/Qwen3-4B",
            "base_model:quantized:Qwen/Qwen3-4B",
            "text-generation",
            "conversational",
        ]);
        let candidates = generation_config_candidates("unsloth/Qwen3-4B-GGUF", &noisy);

        assert!(
            candidates.len() <= crate::services::MAX_GENERATION_CONFIG_LOOKUPS,
            "{candidates:?}"
        );
    }

    /// Fields gglib does not model are ignored rather than tracked, the same
    /// rule `SlotParams` states about `/props`.
    #[test]
    fn unmodelled_fields_are_ignored_without_complaint() {
        let parsed = parse_generation_config(
            r#"{"num_beams": 4, "typical_p": 0.9, "epsilon_cutoff": 0.1, "temperature": 0.6}"#,
        )
        .expect("parses");

        assert_eq!(parsed.config.temperature, Some(0.6));
        assert!(parsed.rejected.is_empty());
    }
}
