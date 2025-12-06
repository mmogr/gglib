//! Unit tests for the update command.

use super::args::UpdateArgs;
use super::metadata_ops::{create_updated_model, parse_metadata_removals, parse_metadata_updates};
use crate::models::Gguf;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

fn create_test_model() -> Gguf {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), "Test Model".to_string());
    metadata.insert("test.key".to_string(), "test.value".to_string());

    Gguf {
        id: Some(1),
        name: "Original Name".to_string(),
        file_path: PathBuf::from("/test/model.gguf"),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    }
}

#[test]
fn test_parse_metadata_updates() {
    let metadata_args = vec![
        "key1=value1".to_string(),
        "key2=value2".to_string(),
        "complex.key=complex value with spaces".to_string(),
    ];

    let result = parse_metadata_updates(&metadata_args).unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result.get("key1"), Some(&"value1".to_string()));
    assert_eq!(result.get("key2"), Some(&"value2".to_string()));
    assert_eq!(
        result.get("complex.key"),
        Some(&"complex value with spaces".to_string())
    );
}

#[test]
fn test_parse_metadata_updates_invalid_format() {
    let metadata_args = vec!["invalid_format".to_string()];
    let result = parse_metadata_updates(&metadata_args);
    assert!(result.is_err());
}

#[test]
fn test_parse_metadata_removals() {
    let remove_arg = Some("key1,key2, key3 ".to_string());
    let result = parse_metadata_removals(&remove_arg).unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result, vec!["key1", "key2", "key3"]);
}

#[test]
fn test_create_updated_model() {
    let existing = create_test_model();
    let args = UpdateArgs {
        id: 1,
        name: Some("Updated Name".to_string()),
        param_count: Some(13.0),
        architecture: Some("mistral".to_string()),
        quantization: None,
        context_length: Some(8192),
        metadata: vec!["new.key=new.value".to_string()],
        remove_metadata: Some("test.key".to_string()),
        replace_metadata: false,
        dry_run: false,
        force: false,
    };

    let metadata_updates = parse_metadata_updates(&args.metadata).unwrap();
    let metadata_removals = parse_metadata_removals(&args.remove_metadata).unwrap();

    let updated =
        create_updated_model(&existing, &args, &metadata_updates, &metadata_removals).unwrap();

    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.param_count_b, 13.0);
    assert_eq!(updated.architecture, Some("mistral".to_string()));
    assert_eq!(updated.quantization, Some("Q4_0".to_string())); // Unchanged
    assert_eq!(updated.context_length, Some(8192));

    // Metadata should have the original general.name, new key added, and test.key removed
    assert_eq!(updated.metadata.len(), 2);
    assert!(updated.metadata.contains_key("general.name"));
    assert!(updated.metadata.contains_key("new.key"));
    assert!(!updated.metadata.contains_key("test.key"));
}

#[test]
fn test_create_updated_model_replace_metadata() {
    let existing = create_test_model();
    let args = UpdateArgs {
        id: 1,
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec!["only.key=only.value".to_string()],
        remove_metadata: None,
        replace_metadata: true,
        dry_run: false,
        force: false,
    };

    let metadata_updates = parse_metadata_updates(&args.metadata).unwrap();
    let metadata_removals = parse_metadata_removals(&args.remove_metadata).unwrap();

    let updated =
        create_updated_model(&existing, &args, &metadata_updates, &metadata_removals).unwrap();

    // With replace_metadata=true, should only have the new metadata
    assert_eq!(updated.metadata.len(), 1);
    assert_eq!(
        updated.metadata.get("only.key"),
        Some(&"only.value".to_string())
    );
    assert!(!updated.metadata.contains_key("general.name"));
    assert!(!updated.metadata.contains_key("test.key"));
}
