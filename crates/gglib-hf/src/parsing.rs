//! JSON parsing functions for `HuggingFace` API responses.
//!
//! This module provides sync parsing functions that convert raw JSON
//! responses into typed domain objects.

use crate::error::{HfError, HfResult};
use crate::kind::keeps;
use crate::models::{HfEntryType, HfFileEntry};
use gglib_core::ports::huggingface::{
    HfModelKind, HfQuantInfo, HfRepoInfo, largest_weights, match_variant, names_contradict,
    sized_parameters_b, variants_of,
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
    listed_repo_info(json, None)
}

/// [`repo_info_from_json`], for a repository whose files were sized as
/// `variants`, with their sizes, when its search answer contradicted the
/// Hub's count ([`names_contradict_the_hub`]).
///
/// The Hub fills `gguf` from one file of the repository, which can be a
/// draft or an MTP head's. With the files listed, the count is the one
/// [`sized_parameters_b`] judges by their sizes; then the safetensors count
/// and the config's, which a GGUF repository rarely has. The Hub's
/// `context_length` goes with its count. Unlisted, the count is the Hub's.
pub(crate) fn listed_repo_info(
    json: &Value,
    variants: Option<&[HfQuantInfo]>,
) -> Option<HfRepoInfo> {
    let text = |path: &[&str]| at(json, path).and_then(Value::as_str).map(str::to_owned);
    let count = |key: &str| json.get(key).and_then(Value::as_u64).unwrap_or(0);
    let billions = |path: &[&str]| at(json, path).and_then(Value::as_f64).map(|n| n / 1e9);

    let model_id = text(&["id"]).filter(|id| !id.is_empty())?;

    let hub = hub_total(json);
    // With no weights to judge by, the Hub's stands.
    let hub_count = match variants {
        Some(variants) if largest_weights(variants).is_some() => sized_parameters_b(hub, variants),
        _ => hub,
    };
    let rejected = hub.is_some() && hub_count != hub;

    // The GGUF header's own count first: it is the one a GGUF repository
    // has. The other two are what a repository of safetensors weights gives.
    let parameters_b = hub_count
        .or_else(|| billions(&["safetensors", "total"]))
        .or_else(|| billions(&["config", "num_parameters"]));

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
        context_length: at(json, &["gguf", "context_length"])
            .and_then(Value::as_u64)
            .filter(|_| !rejected),
        tags,
        model_id,
        file_paths: sibling_paths(json),
    })
}

/// The Hub's count, in billions: its `gguf.total`.
fn hub_total(json: &Value) -> Option<f64> {
    at(json, &["gguf", "total"])
        .and_then(Value::as_f64)
        .map(|n| n / 1e9)
}

/// The paths of the GGUF files a model object names (`siblings`).
pub(crate) fn gguf_sibling_paths(json: &Value) -> Vec<String> {
    sibling_paths(json)
        .into_iter()
        .filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
        })
        .collect()
}

/// The paths of the files a model object names (`siblings`).
fn sibling_paths(json: &Value) -> Vec<String> {
    json.get("siblings")
        .and_then(Value::as_array)
        .map(|siblings| {
            siblings
                .iter()
                .filter_map(|file| file.get("rfilename").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a search hit's model file names contradict the Hub's count:
/// one states more than twice the Hub's `gguf.total`, or states a count
/// where the Hub gives none. Only such a hit's files are sized, to judge
/// the two by their sizes; any other keeps the Hub's count unasked.
pub(crate) fn names_contradict_the_hub(hit: &Value) -> bool {
    let paths = sibling_paths(hit);
    let named = variants_of(paths.iter().map(|path| (path.as_str(), 0)));
    names_contradict(hub_total(hit), &named)
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

/// The hits of a search page that name an ID, hold a GGUF file and are of
/// `kind`, each with that ID.
pub(crate) fn search_hits(hits: &[Value], kind: HfModelKind) -> Vec<(&str, &Value)> {
    hits.iter()
        .filter_map(|hit| {
            let id = hit.get("id").and_then(Value::as_str)?;
            (!id.is_empty()).then_some((id, hit))
        })
        .filter(|(_, hit)| holds_a_gguf_file(hit))
        .filter(|(_, hit)| keeps(kind, hit))
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
