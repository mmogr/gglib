//! JSON parsing functions for `HuggingFace` API responses.
//!
//! This module provides sync parsing functions that convert raw JSON
//! responses into typed domain objects.

use crate::error::{HfError, HfResult};
use crate::models::{HfEntryType, HfFileEntry, HfModelSummary, HfQuantization, HfSearchResponse};
use gglib_core::Quantization;
use serde_json::Value;
use std::collections::HashMap;

// ============================================================================
// Model Summary Parsing
// ============================================================================

/// Parse a single model JSON object into an `HfModelSummary`.
///
/// Returns None if the model doesn't have the required fields or
/// doesn't contain actual GGUF files.
pub fn parse_model_summary(json: &Value) -> Option<HfModelSummary> {
    // Check if the model actually contains .gguf files
    let has_gguf_files = json
        .get("siblings")
        .and_then(|s| s.as_array())
        .is_some_and(|siblings| {
            siblings.iter().any(|file| {
                file.get("rfilename")
                    .and_then(|f| f.as_str())
                    .is_some_and(|name| {
                        std::path::Path::new(name)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
                    })
            })
        });

    if !has_gguf_files {
        return None;
    }

    let id = json.get("id").and_then(|v| v.as_str())?.to_string();

    if id.is_empty() {
        return None;
    }

    // Extract author from id (format: "author/model-name")
    let author = id.split('/').next().map(std::string::ToString::to_string);

    // Extract model name (last part of id)
    let name = id.split('/').next_back().unwrap_or(&id).to_string();

    // Extract parameter count from gguf.total
    #[allow(clippy::cast_precision_loss)] // Precision loss acceptable for parameter display
    let parameters_b = json
        .get("gguf")
        .and_then(|s| s.get("total"))
        .and_then(serde_json::Value::as_u64)
        .map(|params| params as f64 / 1_000_000_000.0);

    let downloads = json
        .get("downloads")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let likes = json
        .get("likes")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let last_modified = json
        .get("lastModified")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);

    let description = json.get("description").and_then(|v| v.as_str()).map(|s| {
        // Truncate long descriptions
        if s.len() > 200 {
            format!("{}...", &s[..197])
        } else {
            s.to_string()
        }
    });

    let tags = json
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(std::string::ToString::to_string))
                .collect()
        })
        .unwrap_or_default();

    Some(HfModelSummary {
        id,
        name,
        author,
        downloads,
        likes,
        last_modified,
        parameters_b,
        description,
        tags,
    })
}

/// Parse a list of model JSON objects into `HfModelSummary` items.
pub fn parse_model_list(json_array: &[Value]) -> Vec<HfModelSummary> {
    json_array.iter().filter_map(parse_model_summary).collect()
}

/// Parse a search response including pagination info.
pub fn parse_search_response(json_array: &[Value], has_more: bool, page: u32) -> HfSearchResponse {
    HfSearchResponse {
        items: parse_model_list(json_array),
        has_more,
        page,
    }
}

// ============================================================================
// Tree Entry Parsing
// ============================================================================

/// Parse a tree/file listing response into `HfFileEntry` items.
pub fn parse_tree_entries(json: &Value) -> HfResult<Vec<HfFileEntry>> {
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

            Some(HfFileEntry {
                path,
                entry_type,
                size,
            })
        })
        .collect();

    Ok(entries)
}

// ============================================================================
// Quantization Aggregation
// ============================================================================

/// Aggregate file entries into quantization groups.
///
/// Groups GGUF files by their quantization type, handling both single files
/// and sharded models (multiple files per quantization).
pub fn aggregate_quantizations(files: &[HfFileEntry]) -> Vec<HfQuantization> {
    let mut quant_map: HashMap<String, HfQuantization> = HashMap::new();

    for file in files {
        if !file.is_gguf() {
            continue;
        }

        // Use gglib-core's Quantization::from_filename
        let quant = Quantization::from_filename(&file.path);
        if quant.is_unknown() {
            continue;
        }

        let quant_name = quant.to_string();

        if let Some(existing) = quant_map.get_mut(&quant_name) {
            // Add to existing quantization (sharded model)
            existing.paths.push(file.path.clone());
            existing.shard_count += 1;
            existing.total_size += file.size;
        } else {
            // New quantization
            quant_map.insert(
                quant_name.clone(),
                HfQuantization {
                    name: quant_name,
                    shard_count: 1,
                    paths: vec![file.path.clone()],
                    total_size: file.size,
                },
            );
        }
    }

    // Sort paths within each quantization (for consistent shard ordering)
    let mut quantizations: Vec<HfQuantization> = quant_map
        .into_values()
        .map(|mut q| {
            q.paths.sort();
            q
        })
        .collect();

    // Sort by quantization name
    quantizations.sort_by(|a, b| a.name.cmp(&b.name));

    quantizations
}

/// Filter files to only GGUF files matching a specific quantization.
pub fn filter_files_by_quantization(files: &[HfFileEntry], quantization: &str) -> Vec<HfFileEntry> {
    let quant_upper = quantization.to_uppercase();

    let mut matching: Vec<HfFileEntry> = files
        .iter()
        .filter(|f| {
            if !f.is_gguf() {
                return false;
            }
            let file_quant = Quantization::from_filename(&f.path);
            file_quant.to_string().to_uppercase() == quant_upper
        })
        .cloned()
        .collect();

    matching.sort_by(|a, b| a.path.cmp(&b.path));
    matching
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_model_summary_with_all_fields() {
        let json = json!({
            "id": "TheBloke/Llama-2-7B-GGUF",
            "downloads": 50000,
            "likes": 42,
            "lastModified": "2024-01-15T10:30:00Z",
            "siblings": [
                {"rfilename": "llama-2-7b.Q4_K_M.gguf"}
            ],
            "gguf": {
                "total": 7_000_000_000_u64
            },
            "tags": ["llama", "gguf"],
            "description": "A fine model for testing"
        });

        let model = parse_model_summary(&json).unwrap();

        assert_eq!(model.id, "TheBloke/Llama-2-7B-GGUF");
        assert_eq!(model.name, "Llama-2-7B-GGUF");
        assert_eq!(model.author, Some("TheBloke".to_string()));
        assert_eq!(model.downloads, 50000);
        assert_eq!(model.likes, 42);
        assert!(model.parameters_b.is_some());
        assert!((model.parameters_b.unwrap() - 7.0).abs() < 0.1);
        assert_eq!(
            model.description,
            Some("A fine model for testing".to_string())
        );
        assert_eq!(model.tags, vec!["llama", "gguf"]);
    }

    #[test]
    fn test_parse_model_summary_no_gguf_files_returns_none() {
        let json = json!({
            "id": "meta-llama/Llama-3.1-8B",
            "downloads": 100_000,
            "likes": 500,
            "siblings": [
                {"rfilename": "model.safetensors"},
                {"rfilename": "config.json"}
            ]
        });

        assert!(parse_model_summary(&json).is_none());
    }

    #[test]
    fn test_parse_model_summary_missing_id_returns_none() {
        let json = json!({
            "downloads": 1000,
            "likes": 10,
            "siblings": [
                {"rfilename": "model.Q4_K_M.gguf"}
            ]
        });

        assert!(parse_model_summary(&json).is_none());
    }

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

    #[test]
    fn test_aggregate_quantizations_single_files() {
        let files = vec![
            HfFileEntry {
                path: "model-Q4_K_M.gguf".to_string(),
                entry_type: HfEntryType::File,
                size: 4_000_000_000,
            },
            HfFileEntry {
                path: "model-Q8_0.gguf".to_string(),
                entry_type: HfEntryType::File,
                size: 8_000_000_000,
            },
            HfFileEntry {
                path: "README.md".to_string(),
                entry_type: HfEntryType::File,
                size: 1000,
            },
        ];

        let quants = aggregate_quantizations(&files);
        assert_eq!(quants.len(), 2);

        // Should be sorted alphabetically
        assert_eq!(quants[0].name, "Q4_K_M");
        assert_eq!(quants[0].shard_count, 1);
        assert_eq!(quants[0].total_size, 4_000_000_000);

        assert_eq!(quants[1].name, "Q8_0");
        assert_eq!(quants[1].shard_count, 1);
    }

    #[test]
    fn test_aggregate_quantizations_sharded() {
        let files = vec![
            HfFileEntry {
                path: "model-Q8_0-00001-of-00003.gguf".to_string(),
                entry_type: HfEntryType::File,
                size: 4_000_000_000,
            },
            HfFileEntry {
                path: "model-Q8_0-00002-of-00003.gguf".to_string(),
                entry_type: HfEntryType::File,
                size: 4_000_000_000,
            },
            HfFileEntry {
                path: "model-Q8_0-00003-of-00003.gguf".to_string(),
                entry_type: HfEntryType::File,
                size: 4_000_000_000,
            },
        ];

        let quants = aggregate_quantizations(&files);
        assert_eq!(quants.len(), 1);
        assert_eq!(quants[0].name, "Q8_0");
        assert_eq!(quants[0].shard_count, 3);
        assert_eq!(quants[0].total_size, 12_000_000_000);
        assert_eq!(quants[0].paths.len(), 3);

        // Paths should be sorted
        assert!(quants[0].paths[0].contains("00001"));
        assert!(quants[0].paths[2].contains("00003"));
    }

    #[test]
    fn test_filter_files_by_quantization() {
        let files = vec![
            HfFileEntry {
                path: "model-Q4_K_M.gguf".to_string(),
                entry_type: HfEntryType::File,
                size: 4_000_000_000,
            },
            HfFileEntry {
                path: "model-Q8_0.gguf".to_string(),
                entry_type: HfEntryType::File,
                size: 8_000_000_000,
            },
            HfFileEntry {
                path: "README.md".to_string(),
                entry_type: HfEntryType::File,
                size: 1000,
            },
        ];

        let filtered = filter_files_by_quantization(&files, "Q4_K_M");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, "model-Q4_K_M.gguf");

        let filtered = filter_files_by_quantization(&files, "q4_k_m"); // case insensitive
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_parse_search_response() {
        let json_array = vec![
            json!({
                "id": "Org/Model1-GGUF",
                "downloads": 1000,
                "siblings": [{"rfilename": "model.gguf"}]
            }),
            json!({
                "id": "Org/Model2-GGUF",
                "downloads": 2000,
                "siblings": [{"rfilename": "model.gguf"}]
            }),
        ];

        let response = parse_search_response(&json_array, true, 0);
        assert_eq!(response.items.len(), 2);
        assert!(response.has_more);
        assert_eq!(response.page, 0);
    }
}
