//! Shared model-naming policy.
//!
//! Both the local-file import path and the `HuggingFace` download path must
//! resolve the same `models.name` for the same underlying GGUF file. This
//! module is the single place that decision is made.

use super::gguf::GgufMetadata;
use std::path::Path;

/// Stored when no naming signal is available at all.
pub const UNKNOWN_MODEL_NAME: &str = "Unknown Model";

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
