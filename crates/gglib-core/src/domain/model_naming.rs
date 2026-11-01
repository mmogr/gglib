//! Shared model-naming policy.
//!
//! Both the local-file import path and the `HuggingFace` download path must
//! resolve the same `models.name` for the same underlying GGUF file. This
//! module is the single place that decision is made.

use super::Model;
use super::gguf::GgufMetadata;
use std::collections::HashMap;
use std::path::Path;

/// Stored when no naming signal is available at all.
pub(crate) const UNKNOWN_MODEL_NAME: &str = "Unknown Model";

/// Which naming signals are available for a model being added.
///
/// Local imports have no repository id, so they skip the repo rung of the
/// ladder in [`resolve_model_name`] entirely rather than passing a sentinel.
#[derive(Debug, Clone, Copy)]
pub enum NameSource<'a> {
    LocalFile,
    HuggingFace { repo_id: &'a str },
}

/// Strip a `HuggingFace` repo id down to its final path segment.
///
/// `"unsloth/Qwen3-8B-GGUF"` -> `"Qwen3-8B-GGUF"`. A bare id with no `/` is
/// returned unchanged. A trailing `/` yields an empty string, matching
/// `str::split('/').next_back()` semantics. Does **not** strip a `-GGUF`
/// suffix — callers that need the repository name as-is (e.g. search
/// results) should use this directly; [`resolve_model_name`] layers
/// [`strip_gguf_suffix`] on top.
#[must_use]
pub fn repo_short_name(repo_id: &str) -> &str {
    repo_id.rsplit('/').next().unwrap_or(repo_id)
}

/// Strip a trailing `-GGUF` marker, case-insensitively.
#[must_use]
pub fn strip_gguf_suffix(name: &str) -> &str {
    if name.len() > 5 && name[name.len() - 5..].eq_ignore_ascii_case("-gguf") {
        &name[..name.len() - 5]
    } else {
        name
    }
}

/// The `general.name` declared in the GGUF header, or `None` if absent or
/// blank.
#[must_use]
pub fn declared_name(gguf: Option<&GgufMetadata>) -> Option<&str> {
    gguf.and_then(|g| g.metadata.get("general.name"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn file_stem_name(file_path: &Path) -> Option<&str> {
    file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Resolve the `models.name` for a model being added.
///
/// The first non-blank rung wins:
/// 1. `general.name` from the GGUF header
/// 2. the `HuggingFace` repo's short name, with the owner prefix and a
///    trailing `-GGUF` stripped (only when `source` carries a repo id)
/// 3. the file stem
/// 4. [`UNKNOWN_MODEL_NAME`]
#[must_use]
pub fn resolve_model_name(
    gguf: Option<&GgufMetadata>,
    file_path: &Path,
    source: NameSource<'_>,
) -> String {
    if let Some(name) = declared_name(gguf) {
        return name.to_string();
    }

    if let NameSource::HuggingFace { repo_id } = source {
        let short = strip_gguf_suffix(repo_short_name(repo_id)).trim();
        if !short.is_empty() {
            return short.to_string();
        }
    }

    if let Some(stem) = file_stem_name(file_path) {
        return stem.to_string();
    }

    UNKNOWN_MODEL_NAME.to_string()
}

/// One catalogued model, as [`openai_model_ids`] and [`resolve_listed_id`]
/// read it: its catalogue id, its name and its quantization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListedModel<'a> {
    /// The model's id in this machine's catalogue.
    pub id: i64,
    /// The model's name, which another model may share.
    pub name: &'a str,
    /// The model's quantization (`Q4_K_M`), when the catalogue recorded one.
    pub quantization: Option<&'a str>,
}

impl<'a> From<&'a Model> for ListedModel<'a> {
    fn from(model: &'a Model) -> Self {
        Self {
            id: model.id,
            name: &model.name,
            quantization: model.quantization.as_deref(),
        }
    }
}

/// What follows the `@` in a listed id such as `qwen3@Q6_K` or `qwen3@7`.
pub const LISTED_ID_SEPARATOR: char = '@';

/// The id each of `models` is listed under in `/v1/models`, in the order
/// given: what an `OpenAI` client shows in its picker and sends back as
/// `model`.
///
/// Two quantizations of one model share its name, so the name alone cannot
/// be the id of both. A name no other model has is its own id. A shared one
/// is `name@QUANT` (`qwen3@Q6_K`) when no other model of that name has the
/// same quantization, and otherwise `name@<catalogue id>` (`qwen3@7`): the
/// same quantization from two repositories, or no quantization recorded.
/// [`resolve_listed_id`] reads both forms back.
///
/// A quantization serves as a suffix only when it cannot be read as
/// anything else: not empty, not a number, which would read as an id, and
/// without an `@`. Otherwise the id form is used.
///
/// An exact name is looked up before any `@` form, so a suffixed id that is
/// also another model's name (a model literally named `qwen3@Q6_K`) would
/// find that model. Such an id falls back to the next form, and in the end
/// to the bare catalogue id, which a lookup tries before any name.
#[must_use]
pub fn openai_model_ids(models: &[ListedModel<'_>]) -> Vec<String> {
    let taken: std::collections::HashSet<&str> = models.iter().map(|m| m.name).collect();
    let mut names: HashMap<&str, usize> = HashMap::new();
    let mut pairs: HashMap<(&str, &str), usize> = HashMap::new();
    for model in models {
        *names.entry(model.name).or_default() += 1;
        if let Some(quantization) = model.quantization {
            *pairs.entry((model.name, quantization)).or_default() += 1;
        }
    }
    models
        .iter()
        .map(|model| {
            if names[model.name] == 1 {
                return model.name.to_owned();
            }
            let by_quantization = model
                .quantization
                .filter(|&q| usable_suffix(q) && pairs[&(model.name, q)] == 1)
                .map(|q| format!("{}{LISTED_ID_SEPARATOR}{q}", model.name));
            let by_id = format!("{}{LISTED_ID_SEPARATOR}{}", model.name, model.id);
            [by_quantization, Some(by_id)]
                .into_iter()
                .flatten()
                .find(|id| !taken.contains(id.as_str()))
                .unwrap_or_else(|| model.id.to_string())
        })
        .collect()
}

/// Whether `quantization` reads back as itself after an `@`.
fn usable_suffix(quantization: &str) -> bool {
    !quantization.is_empty()
        && !quantization.contains(LISTED_ID_SEPARATOR)
        && quantization.parse::<i64>().is_err()
}

/// What a listed id names after its `@`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListedSuffix<'a> {
    /// `name@7`: the model with catalogue id 7, which must be named `name`.
    Id(i64),
    /// `name@Q6_K`: the one model named `name` with that quantization.
    Quantization(&'a str),
}

/// `identifier` split on its last `@` into the name and what follows it, or
/// `None` when it has no `@` or either side is empty.
#[must_use]
pub fn split_listed_id(identifier: &str) -> Option<(&str, ListedSuffix<'_>)> {
    let (name, suffix) = identifier.rsplit_once(LISTED_ID_SEPARATOR)?;
    if name.is_empty() || suffix.is_empty() {
        return None;
    }
    let suffix = suffix
        .parse::<i64>()
        .map_or(ListedSuffix::Quantization(suffix), ListedSuffix::Id);
    Some((name, suffix))
}

/// The catalogue id of the model a listed id names among `models`.
///
/// Read as [`openai_model_ids`] writes it: `name@<id>` is the model with that
/// id when it is named `name`; `name@QUANT` is the one model named `name`
/// with that quantization, and nothing when two have it. `None` for an
/// identifier with no `@`.
///
/// Only the `@` forms: the numeric id and the exact name are tried first, by
/// [`crate::ports::ModelRepository::get_by_identifier`], so a model whose
/// own name contains an `@` is found by it before this is asked.
#[must_use]
pub fn resolve_listed_id(identifier: &str, models: &[ListedModel<'_>]) -> Option<i64> {
    let (name, suffix) = split_listed_id(identifier)?;
    let mut named = models.iter().filter(|m| m.name == name);
    match suffix {
        ListedSuffix::Id(id) => named.find(|m| m.id == id).map(|m| m.id),
        ListedSuffix::Quantization(q) => {
            let mut matching = named.filter(|m| m.quantization == Some(q));
            let found = matching.next()?;
            matching.next().is_none().then_some(found.id)
        }
    }
}

#[cfg(test)]
#[path = "model_naming_listed_tests.rs"]
mod listed_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn gguf_with_name(name: &str) -> GgufMetadata {
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), name.to_string());
        GgufMetadata {
            metadata,
            ..Default::default()
        }
    }

    #[test]
    fn repo_short_name_strips_owner() {
        assert_eq!(repo_short_name("unsloth/Qwen3-8B-GGUF"), "Qwen3-8B-GGUF");
    }

    #[test]
    fn repo_short_name_bare_id_unchanged() {
        assert_eq!(repo_short_name("Qwen3-8B-GGUF"), "Qwen3-8B-GGUF");
    }

    #[test]
    fn repo_short_name_trailing_slash_is_empty() {
        assert_eq!(repo_short_name("unsloth/"), "");
    }

    #[test]
    fn strip_gguf_suffix_removes_uppercase() {
        assert_eq!(strip_gguf_suffix("Qwen3-8B-GGUF"), "Qwen3-8B");
    }

    #[test]
    fn strip_gguf_suffix_case_insensitive() {
        assert_eq!(strip_gguf_suffix("Qwen3-8B-gguf"), "Qwen3-8B");
        assert_eq!(strip_gguf_suffix("Qwen3-8B-Gguf"), "Qwen3-8B");
    }

    #[test]
    fn strip_gguf_suffix_no_match_unchanged() {
        assert_eq!(strip_gguf_suffix("Qwen3-8B"), "Qwen3-8B");
    }

    #[test]
    fn strip_gguf_suffix_panic_safety() {
        assert_eq!(strip_gguf_suffix(""), "");
        assert_eq!(strip_gguf_suffix("a"), "a");
        assert_eq!(strip_gguf_suffix("-GGUF"), "-GGUF");
        assert_eq!(strip_gguf_suffix("模型-GGUF"), "模型");
    }

    #[test]
    fn declared_name_reads_general_name() {
        let gguf = gguf_with_name("Qwen3-8B");
        assert_eq!(declared_name(Some(&gguf)), Some("Qwen3-8B"));
    }

    #[test]
    fn declared_name_blank_is_none() {
        let gguf = gguf_with_name("   ");
        assert_eq!(declared_name(Some(&gguf)), None);
    }

    #[test]
    fn declared_name_absent_metadata_is_none() {
        assert_eq!(declared_name(None), None);
        assert_eq!(declared_name(Some(&GgufMetadata::default())), None);
    }

    #[test]
    fn resolve_prefers_declared_name_over_repo_and_stem() {
        let gguf = gguf_with_name("Qwen3-8B");
        let name = resolve_model_name(
            Some(&gguf),
            &PathBuf::from("/models/other-file.gguf"),
            NameSource::HuggingFace {
                repo_id: "unsloth/Qwen3.6-27B-MTP-GGUF",
            },
        );
        assert_eq!(name, "Qwen3-8B");
    }

    #[test]
    fn resolve_falls_back_to_repo_short_name_stripped() {
        let name = resolve_model_name(
            None,
            &PathBuf::from("/models/some-file.gguf"),
            NameSource::HuggingFace {
                repo_id: "unsloth/Qwen3.6-27B-MTP-GGUF",
            },
        );
        assert_eq!(name, "Qwen3.6-27B-MTP");
    }

    #[test]
    fn resolve_local_file_skips_repo_rung() {
        let name = resolve_model_name(
            None,
            &PathBuf::from("/models/qwen3-8b-q4_k_m.gguf"),
            NameSource::LocalFile,
        );
        assert_eq!(name, "qwen3-8b-q4_k_m");
    }

    #[test]
    fn resolve_blank_declared_name_falls_through() {
        let gguf = gguf_with_name("  ");
        let name = resolve_model_name(
            Some(&gguf),
            &PathBuf::from("/models/qwen3-8b.gguf"),
            NameSource::LocalFile,
        );
        assert_eq!(name, "qwen3-8b");
    }

    #[test]
    fn resolve_nothing_available_is_unknown() {
        let name = resolve_model_name(None, &PathBuf::from("/"), NameSource::LocalFile);
        assert_eq!(name, UNKNOWN_MODEL_NAME);
    }

    #[test]
    fn resolve_hf_repo_id_ending_in_slash_falls_to_stem() {
        let name = resolve_model_name(
            None,
            &PathBuf::from("/models/qwen3-8b.gguf"),
            NameSource::HuggingFace {
                repo_id: "unsloth/",
            },
        );
        assert_eq!(name, "qwen3-8b");
    }
}
