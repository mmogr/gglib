//! Model type conversions between domain and legacy types.
//!
//! This module handles translation between the clean domain types
//! (`Model`, `NewModel`) and the legacy `Gguf` type used throughout
//! the existing codebase.

use crate::core::domain::{Model, NewModel};
use crate::models::Gguf;

/// Convert a `Gguf` (legacy type) to a `Model` (domain type).
///
/// # Panics
///
/// Panics if the `Gguf` has no ID (use `gguf_to_new_model` instead).
pub fn gguf_to_model(gguf: Gguf) -> Model {
    Model {
        id: gguf.id.expect("Gguf must have an ID to convert to Model") as i64,
        name: gguf.name,
        file_path: gguf.file_path,
        param_count_b: gguf.param_count_b,
        architecture: gguf.architecture,
        quantization: gguf.quantization,
        context_length: gguf.context_length,
        metadata: gguf.metadata,
        added_at: gguf.added_at,
        hf_repo_id: gguf.hf_repo_id,
        hf_commit_sha: gguf.hf_commit_sha,
        hf_filename: gguf.hf_filename,
        download_date: gguf.download_date,
        last_update_check: gguf.last_update_check,
        tags: gguf.tags,
    }
}

/// Convert a `Gguf` to a `NewModel` (drops the ID).
pub fn gguf_to_new_model(gguf: Gguf) -> NewModel {
    NewModel {
        name: gguf.name,
        file_path: gguf.file_path,
        param_count_b: gguf.param_count_b,
        architecture: gguf.architecture,
        quantization: gguf.quantization,
        context_length: gguf.context_length,
        metadata: gguf.metadata,
        added_at: gguf.added_at,
        hf_repo_id: gguf.hf_repo_id,
        hf_commit_sha: gguf.hf_commit_sha,
        hf_filename: gguf.hf_filename,
        download_date: gguf.download_date,
        last_update_check: gguf.last_update_check,
        tags: gguf.tags,
    }
}

/// Convert a `Model` (domain type) to a `Gguf` (legacy type).
pub fn model_to_gguf(model: Model) -> Gguf {
    Gguf {
        id: Some(model.id as u32),
        name: model.name,
        file_path: model.file_path,
        param_count_b: model.param_count_b,
        architecture: model.architecture,
        quantization: model.quantization,
        context_length: model.context_length,
        metadata: model.metadata,
        added_at: model.added_at,
        hf_repo_id: model.hf_repo_id,
        hf_commit_sha: model.hf_commit_sha,
        hf_filename: model.hf_filename,
        download_date: model.download_date,
        last_update_check: model.last_update_check,
        tags: model.tags,
    }
}

/// Convert a `NewModel` (domain type) to a `Gguf` (legacy type).
pub fn new_model_to_gguf(model: NewModel) -> Gguf {
    Gguf {
        id: None,
        name: model.name,
        file_path: model.file_path,
        param_count_b: model.param_count_b,
        architecture: model.architecture,
        quantization: model.quantization,
        context_length: model.context_length,
        metadata: model.metadata,
        added_at: model.added_at,
        hf_repo_id: model.hf_repo_id,
        hf_commit_sha: model.hf_commit_sha,
        hf_filename: model.hf_filename,
        download_date: model.download_date,
        last_update_check: model.last_update_check,
        tags: model.tags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn create_test_gguf(with_id: bool) -> Gguf {
        Gguf {
            id: if with_id { Some(42) } else { None },
            name: "Test Model".to_string(),
            file_path: PathBuf::from("/path/to/model.gguf"),
            param_count_b: 7.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: Some("TheBloke/Model-GGUF".to_string()),
            hf_commit_sha: None,
            hf_filename: Some("model.Q4_0.gguf".to_string()),
            download_date: None,
            last_update_check: None,
            tags: vec!["chat".to_string()],
        }
    }

    #[test]
    fn test_gguf_to_model() {
        let gguf = create_test_gguf(true);
        let model = gguf_to_model(gguf);

        assert_eq!(model.id, 42);
        assert_eq!(model.name, "Test Model");
        assert_eq!(model.architecture, Some("llama".to_string()));
    }

    #[test]
    fn test_gguf_to_new_model() {
        let gguf = create_test_gguf(true);
        let new_model = gguf_to_new_model(gguf);

        assert_eq!(new_model.name, "Test Model");
        assert_eq!(new_model.tags, vec!["chat".to_string()]);
    }

    #[test]
    fn test_roundtrip_model() {
        let gguf = create_test_gguf(true);
        let original_name = gguf.name.clone();

        let model = gguf_to_model(gguf);
        let back_to_gguf = model_to_gguf(model);

        assert_eq!(back_to_gguf.name, original_name);
        assert_eq!(back_to_gguf.id, Some(42));
    }

    #[test]
    fn test_roundtrip_new_model() {
        let gguf = create_test_gguf(false);
        let original_name = gguf.name.clone();

        let new_model = gguf_to_new_model(gguf);
        let back_to_gguf = new_model_to_gguf(new_model);

        assert_eq!(back_to_gguf.name, original_name);
        assert_eq!(back_to_gguf.id, None);
    }
}
