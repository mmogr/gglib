//! JSON parsing functions for `HuggingFace` API responses.
//!
//! This module provides sync parsing functions that convert raw JSON
//! responses into typed domain objects.

use crate::error::{HfError, HfResult};
use crate::kind::keeps;
use crate::models::{HfEntryType, HfFileEntry};
use gglib_core::ports::huggingface::{
    HfModelKind, HfQuantInfo, HfRepoInfo, match_variant, variants_of,
};
use gglib_core::repo_short_name;
use serde_json::Value;

// ============================================================================
// Model Summary Parsing
// ============================================================================

/// The summary of one model object off the Hub, whichever endpoint gave it:
/// a search hit and the model-info answer are read by the same rules, so a
/// repository found by search and one looked up by its ID show the same
/// thing. `None` for an object that names no ID.
///
/// A search asks the Hub for a hit's `siblings`, `gguf`, `likes`, `downloads`
/// and `tags` alone, and the model info carries more; a field a response does
/// not hold is absent from the summary.
pub(crate) fn repo_info_from_json(json: &Value) -> Option<HfRepoInfo> {
    let text = |path: &[&str]| at(json, path).and_then(Value::as_str).map(str::to_owned);
    let count = |key: &str| json.get(key).and_then(Value::as_u64).unwrap_or(0);

    let model_id = text(&["id"]).filter(|id| !id.is_empty())?;

    // The GGUF header's own count first: it is the one a GGUF repository
    // has. The other two are what a repository of safetensors weights gives.
    let parameters_b = [
        &["gguf", "total"][..],
        &["safetensors", "total"],
        &["config", "num_parameters"],
    ]
    .iter()
    .find_map(|path| at(json, path).and_then(Value::as_f64))
    .map(|parameters| parameters / 1_000_000_000.0);

    let tags = json
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| tag.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    Some(HfRepoInfo {
        name: repo_short_name(&model_id).to_owned(),
        author: model_id.split('/').next().map(str::to_owned),
        downloads: count("downloads"),
        likes: count("likes"),
        parameters_b,
        description: text(&["description"])
            .or_else(|| text(&["cardData", "model_summary"]))
            .map(|description| cut_description(&description)),
        last_modified: text(&["lastModified"]),
        chat_template: text(&["gguf", "chat_template"])
            .or_else(|| text(&["config", "chat_template"])),
        architecture: text(&["gguf", "architecture"]),
        context_length: at(json, &["gguf", "context_length"]).and_then(Value::as_u64),
        tags,
        model_id,
    })
}

/// The value at `path` in `json`, one key per level.
fn at<'a>(json: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(json, |value, key| value.get(key))
}

/// A description of at most 200 bytes: a longer one is its first 197, back
/// to a character boundary, and `...`.
fn cut_description(description: &str) -> String {
    if description.len() <= 200 {
        description.to_owned()
    } else {
        let kept = &description[..description.floor_char_boundary(197)];
        format!("{kept}...")
    }
}

/// The hits of a search page that hold a GGUF file and are of `kind`, each as
/// its summary.
pub(crate) fn search_hits(hits: &[Value], kind: HfModelKind) -> Vec<HfRepoInfo> {
    hits.iter()
        .filter(|hit| holds_a_gguf_file(hit))
        .filter(|hit| keeps(kind, hit))
        .filter_map(repo_info_from_json)
        .collect()
}

/// Whether a search hit lists a `.gguf` file among its siblings.
fn holds_a_gguf_file(hit: &Value) -> bool {
    hit.get("siblings")
        .and_then(Value::as_array)
        .is_some_and(|siblings| {
            siblings.iter().any(|file| {
                file.get("rfilename")
                    .and_then(Value::as_str)
                    .is_some_and(|name| {
                        std::path::Path::new(name)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
                    })
            })
        })
}

// ============================================================================
// Tree Entry Parsing
// ============================================================================

/// Parse a tree/file listing response into `HfFileEntry` items.
pub(crate) fn parse_tree_entries(json: &Value) -> HfResult<Vec<HfFileEntry>> {
    let array = json.as_array().ok_or_else(|| HfError::InvalidResponse {
        message: "Expected array for tree response".to_string(),
    })?;

    let entries = array
        .iter()
        .filter_map(|item| {
            let path = item.get("path").and_then(|v| v.as_str())?.to_string();
            let entry_type = match item.get("type").and_then(|v| v.as_str()) {
                Some("directory") => HfEntryType::Directory,
                _ => HfEntryType::File,
            };
            let size = item
                .get("size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            // Prefer lfs.oid (SHA256 of file content) over top-level oid (Git SHA-1).
            // The top-level oid is a Git object hash and cannot be used for file verification.
            let oid = item
                .get("lfs")
                .and_then(|lfs| lfs.get("oid"))
                .and_then(|v| v.as_str())
                .or_else(|| item.get("oid").and_then(|v| v.as_str()))
                .map(ToString::to_string);

            Some(HfFileEntry {
                path,
                entry_type,
                size,
                oid,
            })
        })
        .collect();

    Ok(entries)
}

// ============================================================================
// Variants
// ============================================================================

/// The repository's variants among `files`: its GGUF files grouped into
/// shard sets and labelled as [`variants_of`] does. No header is read.
pub(crate) fn variants(files: &[HfFileEntry]) -> Vec<HfQuantInfo> {
    variants_of(
        files
            .iter()
            .filter(|file| file.is_gguf())
            .map(|file| (file.path.as_str(), file.size)),
    )
}

/// The files, in shard order, of the variant among `files` that `label`
/// names as [`match_variant`] reads a request; `None` when it names no single
/// one.
pub(crate) fn variant_files(files: &[HfFileEntry], label: &str) -> Option<Vec<HfFileEntry>> {
    let variants = variants(files);
    let variant = match_variant(&variants, label).ok()?;
    Some(
        variant
            .file_paths
            .iter()
            .filter_map(|path| files.iter().find(|file| &file.path == path).cloned())
            .collect(),
    )
}

#[cfg(test)]
#[path = "summary_tests.rs"]
pub(crate) mod summary_tests;

#[cfg(test)]
#[path = "variants_tests.rs"]
mod variants_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_tree_entries_mixed() {
        let json = json!([
            {"path": "README.md", "type": "file", "size": 1000},
            {"path": "model.Q4_K_M.gguf", "type": "file", "size": 4_000_000_000_u64},
            {"path": "subdir", "type": "directory", "size": 0}
        ]);

        let entries = parse_tree_entries(&json).unwrap();
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].path, "README.md");
        assert!(!entries[0].is_gguf());

        assert_eq!(entries[1].path, "model.Q4_K_M.gguf");
        assert!(entries[1].is_gguf());
        assert_eq!(entries[1].size, 4_000_000_000);

        assert_eq!(entries[2].path, "subdir");
        assert!(entries[2].is_directory());
    }

    #[test]
    fn test_parse_tree_entries_invalid_json() {
        let json = json!({"not": "an array"});
        assert!(parse_tree_entries(&json).is_err());
    }
}
