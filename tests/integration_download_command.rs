//! Integration tests for the download command functionality.
//!
//! This module tests the download workflow including:
//! - Model repository discovery
//! - File quantization detection
//! - Download workflow simulation
//! - Database integration with HuggingFace metadata

mod common;

use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::tempdir;

use gglib::models::Gguf;
use gglib::services::database;

/// Create an isolated test database pool with the proper schema
async fn create_test_pool() -> Result<SqlitePool> {
    common::database::setup_test_pool().await
}

/// Create a test model simulating a downloaded model from HuggingFace
fn create_downloaded_model(repo_id: &str, quantization: &str) -> Gguf {
    let mut metadata = HashMap::new();
    metadata.insert(
        "general.name".to_string(),
        "Downloaded Test Model".to_string(),
    );
    metadata.insert("general.architecture".to_string(), "llama".to_string());
    metadata.insert("hf_source".to_string(), repo_id.to_string());

    let filename = format!("model-{}.gguf", quantization.to_lowercase());
    let repo_scoped_path = format!("/test/models/{repo_id}/{filename}");

    Gguf {
        id: None,
        name: format!("Test Model ({})", quantization),
        file_path: PathBuf::from(repo_scoped_path),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some(quantization.to_string()),
        context_length: Some(4096),
        metadata,
        added_at: Utc::now(),
        hf_repo_id: Some(repo_id.to_string()),
        hf_commit_sha: Some("abc123def456".to_string()),
        hf_filename: Some(filename),
        download_date: Some(Utc::now()),
        last_update_check: None,
        tags: Vec::new(),
    }
}

#[tokio::test]
async fn test_download_model_database_integration() {
    let pool = create_test_pool().await.unwrap();
    let model = create_downloaded_model("test/llama-7b", "Q4_K_M");

    // Test adding downloaded model to database
    let result = database::add_model(&pool, &model).await;
    assert!(result.is_ok(), "Adding downloaded model should succeed");

    // Verify model was added with HuggingFace metadata
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 1);

    let added_model = &models[0];
    assert_eq!(added_model.hf_repo_id, Some("test/llama-7b".to_string()));
    assert_eq!(added_model.hf_commit_sha, Some("abc123def456".to_string()));
    assert_eq!(
        added_model.hf_filename,
        Some("model-q4_k_m.gguf".to_string())
    );
    assert!(added_model.download_date.is_some());
    assert_eq!(added_model.quantization, Some("Q4_K_M".to_string()));
}

#[tokio::test]
async fn test_download_model_with_various_quantizations() {
    let pool = create_test_pool().await.unwrap();

    let quantizations = vec!["Q4_K_M", "Q8_0", "F16", "Q4_0"];

    for quant in quantizations {
        let model = create_downloaded_model("test/model", quant);
        database::add_model(&pool, &model).await.unwrap();
    }

    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 4);

    // Verify each model has correct quantization
    for model in &models {
        assert!(model.quantization.is_some());
        assert!(model.hf_repo_id.is_some());
        assert!(model.download_date.is_some());
    }
}

#[tokio::test]
async fn test_downloaded_model_update_tracking() {
    let pool = create_test_pool().await.unwrap();
    let mut model = create_downloaded_model("test/updateable-model", "Q8_0");

    // Initial download
    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    // Simulate update check
    model.last_update_check = Some(Utc::now());
    database::update_model(&pool, model_id, &model)
        .await
        .unwrap();

    // Verify update tracking
    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();
    assert!(updated_model.last_update_check.is_some());
    assert_eq!(
        updated_model.hf_repo_id,
        Some("test/updateable-model".to_string())
    );
}

#[test]
fn test_quantization_extraction_from_filename() {
    // Test the quantization extraction logic that would be used in download
    let test_cases = vec![
        ("model-Q4_K_M.gguf", "Q4_K_M"),
        ("llama-7b-Q8_0.gguf", "Q8_0"),
        ("model-f16.gguf", "F16"),
        ("model-Q4_0.gguf", "Q4_0"),
        ("model-q6_k.gguf", "Q6_K"),
        ("random-name.gguf", "unknown"),
    ];

    for (filename, expected) in test_cases {
        let extracted = extract_quantization_from_filename_test_helper(filename);
        assert_eq!(extracted, expected, "Failed for filename: {}", filename);
    }
}

// Helper function to test quantization extraction (mirrors model_ops.rs logic)
fn extract_quantization_from_filename_test_helper(filename: &str) -> &str {
    if filename.contains("q8_0") || filename.contains("Q8_0") {
        "Q8_0"
    } else if filename.contains("q8") || filename.contains("Q8") {
        "Q8"
    } else if filename.contains("q4_k_m") || filename.contains("Q4_K_M") {
        "Q4_K_M"
    } else if filename.contains("q4_0") || filename.contains("Q4_0") {
        "Q4_0"
    } else if filename.contains("q4") || filename.contains("Q4") {
        "Q4"
    } else if filename.contains("q6_k") || filename.contains("Q6_K") {
        "Q6_K"
    } else if filename.contains("q6") || filename.contains("Q6") {
        "Q6"
    } else if filename.contains("f16")
        || filename.contains("F16")
        || filename.contains("fp16")
        || filename.contains("FP16")
    {
        "F16"
    } else if filename.contains("f32")
        || filename.contains("F32")
        || filename.contains("fp32")
        || filename.contains("FP32")
    {
        "F32"
    } else {
        "unknown"
    }
}

#[test]
fn test_model_directory_sanitization() {
    // Test model name sanitization for directory creation
    let test_cases = vec![
        ("microsoft/DialoGPT-medium", "microsoft_DialoGPT-medium"),
        ("meta-llama/Llama-2-7b-chat", "meta-llama_Llama-2-7b-chat"),
        ("model:with:colons", "model_with_colons"),
        ("path/with\\backslash", "path_with_backslash"),
        ("normal-model-name", "normal-model-name"),
    ];

    for (input, expected) in test_cases {
        let sanitized = sanitize_model_name_test_helper(input);
        assert_eq!(sanitized, expected, "Failed for input: {}", input);
    }
}

// Helper function to test model name sanitization (mirrors utils.rs logic)
fn sanitize_model_name_test_helper(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_")
}

#[tokio::test]
async fn test_download_error_handling() {
    let pool = create_test_pool().await.unwrap();

    // Test handling of model without HuggingFace metadata
    let mut model = create_downloaded_model("test/error-model", "Q4_0");
    model.hf_repo_id = None;

    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    let non_hf_model = &models[0];

    // Model should be stored but won't be updateable since it's not from HF
    assert!(non_hf_model.hf_repo_id.is_none());
    assert!(non_hf_model.download_date.is_some()); // Still has download date
}

#[tokio::test]
async fn test_multiple_versions_same_model() {
    let pool = create_test_pool().await.unwrap();

    // Simulate downloading different quantizations of the same model
    let repo_id = "test/multi-quant-model";
    let quantizations = vec!["Q4_K_M", "Q8_0", "F16"];

    for quant in quantizations {
        let model = create_downloaded_model(repo_id, quant);
        database::add_model(&pool, &model).await.unwrap();
    }

    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 3);

    // All should have same repo_id but different quantizations
    for model in &models {
        assert_eq!(model.hf_repo_id, Some(repo_id.to_string()));
        assert!(model.quantization.is_some());
    }

    // Should be able to find by name pattern (searches "Test Model" which should match all)
    let repo_models = database::find_models_by_name(&pool, "Test Model")
        .await
        .unwrap();
    assert_eq!(repo_models.len(), 3);
}

#[test]
fn test_models_directory_env_override_integration() {
    use gglib::commands::download::get_models_directory;

    let temp = tempdir().unwrap();
    let custom = temp.path().join("integrated");
    unsafe {
        std::env::set_var("GGLIB_MODELS_DIR", &custom);
    }

    let dir = get_models_directory().unwrap();
    assert_eq!(dir, custom);
    assert!(dir.exists());

    unsafe {
        std::env::remove_var("GGLIB_MODELS_DIR");
    }
}

#[tokio::test]
async fn test_download_timestamp_tracking() {
    let pool = create_test_pool().await.unwrap();

    let now = Utc::now();
    let mut model = create_downloaded_model("test/timestamp-model", "Q4_0");
    model.download_date = Some(now);

    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    let retrieved = &models[0];

    // Verify timestamp is preserved (allowing for small differences due to serialization)
    assert!(retrieved.download_date.is_some());
    let retrieved_time = retrieved.download_date.unwrap();
    let diff = (retrieved_time.timestamp() - now.timestamp()).abs();
    assert!(diff < 2, "Timestamp difference too large: {} seconds", diff);
}

// =============================================================================
// New HuggingFace Integration Tests
// =============================================================================

#[tokio::test]
async fn test_quantization_detection_modern_formats() {
    use gglib::commands::download::extract_quantization_from_filename;

    // Test all modern quantization formats
    let test_cases = vec![
        // 1-bit quantizations
        ("model-IQ1_S.gguf", "IQ1_S"),
        ("model-IQ1_M.gguf", "IQ1_M"),
        // 2-bit quantizations
        ("model-IQ2_XXS.gguf", "IQ2_XXS"),
        ("model-IQ2_XS.gguf", "IQ2_XS"),
        ("model-IQ2_S.gguf", "IQ2_S"),
        ("model-IQ2_M.gguf", "IQ2_M"),
        ("model-Q2_K.gguf", "Q2_K"),
        ("model-Q2_K_L.gguf", "Q2_K_L"),
        ("model-Q2_K_XL.gguf", "Q2_K_XL"),
        // 3-bit quantizations
        ("model-IQ3_XXS.gguf", "IQ3_XXS"),
        ("model-IQ3_XS.gguf", "IQ3_XS"),
        ("model-IQ3_M.gguf", "IQ3_M"),
        ("model-Q3_K_S.gguf", "Q3_K_S"),
        ("model-Q3_K_M.gguf", "Q3_K_M"),
        ("model-Q3_K_L.gguf", "Q3_K_L"),
        ("model-Q3_K_XL.gguf", "Q3_K_XL"),
        // 4-bit quantizations
        ("model-IQ4_XS.gguf", "IQ4_XS"),
        ("model-IQ4_NL.gguf", "IQ4_NL"),
        ("model-Q4_0.gguf", "Q4_0"),
        ("model-Q4_1.gguf", "Q4_1"),
        ("model-Q4_K_S.gguf", "Q4_K_S"),
        ("model-Q4_K_M.gguf", "Q4_K_M"),
        ("model-Q4_K_L.gguf", "Q4_K_L"),
        ("model-Q4_K_XL.gguf", "Q4_K_XL"),
        ("model-MXFP4.gguf", "MXFP4"),
        // 5-bit quantizations
        ("model-Q5_0.gguf", "Q5_0"),
        ("model-Q5_1.gguf", "Q5_1"),
        ("model-Q5_K_S.gguf", "Q5_K_S"),
        ("model-Q5_K_M.gguf", "Q5_K_M"),
        ("model-Q5_K_L.gguf", "Q5_K_L"),
        ("model-Q5_K_XL.gguf", "Q5_K_XL"),
        // 6-bit quantizations
        ("model-Q6_K.gguf", "Q6_K"),
        ("model-Q6_K_L.gguf", "Q6_K_L"),
        ("model-Q6_K_XL.gguf", "Q6_K_XL"),
        // 8-bit quantizations
        ("model-Q8_0.gguf", "Q8_0"),
        ("model-Q8_K_XL.gguf", "Q8_K_XL"),
        // 16-bit and special formats
        ("model-F16.gguf", "F16"),
        ("model-BF16.gguf", "BF16"),
        ("model-F32.gguf", "F32"),
        ("model-imatrix.gguf", "imatrix"),
    ];

    for (filename, expected) in test_cases {
        let result = extract_quantization_from_filename(filename);
        assert_eq!(
            result, expected,
            "Failed for filename '{}': expected '{}', got '{}'",
            filename, expected, result
        );
    }
}

#[tokio::test]
async fn test_quantization_detection_case_insensitive() {
    use gglib::commands::download::extract_quantization_from_filename;

    // Test case insensitivity
    let test_cases = vec![
        ("MODEL-q4_k_m.gguf", "Q4_K_M"),
        ("model-Q4_K_M.gguf", "Q4_K_M"),
        ("Model-Q4_k_M.GGUF", "Q4_K_M"),
        ("model-iq2_xxs.gguf", "IQ2_XXS"),
        ("MODEL-IQ2_XXS.GGUF", "IQ2_XXS"),
        ("model-f16.gguf", "F16"),
        ("MODEL-BF16.GGUF", "BF16"),
    ];

    for (filename, expected) in test_cases {
        let result = extract_quantization_from_filename(filename);
        assert_eq!(
            result, expected,
            "Case insensitive test failed for '{}': expected '{}', got '{}'",
            filename, expected, result
        );
    }
}

#[tokio::test]
async fn test_quantization_detection_with_prefixes() {
    use gglib::commands::download::extract_quantization_from_filename;

    // Test quantization detection with various prefixes (like UD- for Ultra Dense)
    let test_cases = vec![
        ("Meta-Llama-3.1-70B-Instruct.Q6_K.gguf", "Q6_K"),
        ("Meta-Llama-3.1-70B-Instruct.IQ4_NL.gguf", "IQ4_NL"),
        ("DeepSeek-R1-Distill-Llama-70B-UD-IQ1_M.gguf", "IQ1_M"),
        ("DeepSeek-R1-Distill-Llama-70B-UD-Q2_K_XL.gguf", "Q2_K_XL"),
        ("complex-model-name-with-dashes-Q4_K_M.gguf", "Q4_K_M"),
        ("Model_With_Underscores_IQ3_XXS.gguf", "IQ3_XXS"),
    ];

    for (filename, expected) in test_cases {
        let result = extract_quantization_from_filename(filename);
        assert_eq!(
            result, expected,
            "Prefix test failed for '{}': expected '{}', got '{}'",
            filename, expected, result
        );
    }
}

#[tokio::test]
async fn test_sharded_quantization_detection() {
    use gglib::commands::download::extract_quantization_from_filename;

    // Test sharded file quantization detection
    let test_cases = vec![
        (
            "Meta-Llama-3.1-70B-Instruct.Q6_K.gguf-00001-of-00006.gguf",
            "Q6_K",
        ),
        (
            "Meta-Llama-3.1-70B-Instruct.Q6_K.gguf-00002-of-00006.gguf",
            "Q6_K",
        ),
        (
            "Meta-Llama-3.1-70B-Instruct.Q6_K.gguf-00006-of-00006.gguf",
            "Q6_K",
        ),
        ("model-IQ4_NL.gguf-00001-of-00003.gguf", "IQ4_NL"),
        ("model-Q8_0.gguf-part-01-of-05.gguf", "Q8_0"),
        ("Large-Model.BF16.gguf-shard-1-of-10.gguf", "BF16"),
    ];

    for (filename, expected) in test_cases {
        let result = extract_quantization_from_filename(filename);
        assert_eq!(
            result, expected,
            "Sharded file test failed for '{}': expected '{}', got '{}'",
            filename, expected, result
        );
    }
}

#[tokio::test]
async fn test_quantization_precedence() {
    use gglib::commands::download::extract_quantization_from_filename;

    // Test that more specific patterns take precedence over general ones
    let test_cases = vec![
        // Should detect Q4_K_M, not just Q4
        ("model-Q4_K_M.gguf", "Q4_K_M"),
        ("model-Q4_K_M-other-Q4.gguf", "Q4_K_M"),
        // Should detect Q2_K_XL, not just Q2_K
        ("model-Q2_K_XL.gguf", "Q2_K_XL"),
        ("model-Q2_K_XL-extra-Q2_K.gguf", "Q2_K_XL"),
        // Should detect IQ2_XXS, not just IQ2_XS
        ("model-IQ2_XXS.gguf", "IQ2_XXS"),
        // Should detect Q8_K_XL, not just Q8_0 or Q8
        ("model-Q8_K_XL.gguf", "Q8_K_XL"),
    ];

    for (filename, expected) in test_cases {
        let result = extract_quantization_from_filename(filename);
        assert_eq!(
            result, expected,
            "Precedence test failed for '{}': expected '{}', got '{}'",
            filename, expected, result
        );
    }
}

#[tokio::test]
async fn test_unknown_quantization_formats() {
    use gglib::commands::download::extract_quantization_from_filename;

    // Test files that should return "unknown"
    let test_cases = vec![
        "model.gguf",
        "model-unknown-format.gguf",
        "model-custom-quant.gguf",
        "model-experimental.gguf",
        "README.md",
        "config.json",
        "model.safetensors",
        "",
        "model-Q99_EXPERIMENTAL.gguf", // Non-standard quantization
    ];

    for filename in test_cases {
        let result = extract_quantization_from_filename(filename);
        assert_eq!(
            result, "unknown",
            "Unknown test failed for '{}': expected 'unknown', got '{}'",
            filename, result
        );
    }
}

#[tokio::test]
async fn test_hf_sharded_model_database_integration() {
    let pool = create_test_pool().await.unwrap();

    // Create a sharded model entry
    let mut model =
        create_downloaded_model("MaziyarPanahi/Meta-Llama-3.1-70B-Instruct-GGUF", "Q6_K");
    model.quantization = Some("Q6_K (sharded: 6 parts)".to_string());
    model.hf_filename =
        Some("Meta-Llama-3.1-70B-Instruct.Q6_K.gguf-00001-of-00006.gguf".to_string());

    // Test adding sharded model to database
    let result = database::add_model(&pool, &model).await;
    assert!(result.is_ok(), "Adding sharded model should succeed");

    // Verify model was added with sharding notation
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 1);

    let added_model = &models[0];
    assert_eq!(
        added_model.hf_repo_id,
        Some("MaziyarPanahi/Meta-Llama-3.1-70B-Instruct-GGUF".to_string())
    );
    assert_eq!(
        added_model.quantization,
        Some("Q6_K (sharded: 6 parts)".to_string())
    );
    assert!(
        added_model
            .hf_filename
            .as_ref()
            .unwrap()
            .contains("00001-of-00006")
    );
}

#[tokio::test]
async fn test_hf_model_with_modern_quantizations() {
    let pool = create_test_pool().await.unwrap();

    // Test modern quantization formats in database
    let modern_quants = vec![
        "IQ1_S", "IQ1_M", "IQ2_XXS", "IQ2_XS", "IQ2_S", "IQ2_M", "IQ3_XXS", "IQ3_XS", "IQ3_M",
        "IQ4_XS", "IQ4_NL", "Q2_K_L", "Q2_K_XL", "Q3_K_L", "Q3_K_XL", "Q4_K_L", "Q4_K_XL",
        "Q5_K_L", "Q5_K_XL", "Q6_K_L", "Q6_K_XL", "Q8_K_XL", "MXFP4", "BF16", "imatrix",
    ];

    for quant in &modern_quants {
        let model = create_downloaded_model("test/modern-model", quant);
        database::add_model(&pool, &model).await.unwrap();
    }

    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), modern_quants.len());

    // Verify each model has correct quantization
    for model in &models {
        assert!(model.quantization.is_some());
        assert!(modern_quants.contains(&model.quantization.as_ref().unwrap().as_str()));
        assert!(model.hf_repo_id.is_some());
        assert!(model.download_date.is_some());
    }
}

#[tokio::test]
async fn test_hf_repo_id_validation() {
    let pool = create_test_pool().await.unwrap();

    // Test various HuggingFace repository ID formats
    let valid_repo_ids = vec![
        "microsoft/DialoGPT-medium",
        "meta-llama/Llama-3.1-70B-Instruct",
        "unsloth/DeepSeek-R1-Distill-Llama-70B-GGUF",
        "bartowski/Llama-3.2-3B-Instruct-GGUF",
        "MaziyarPanahi/Meta-Llama-3.1-70B-Instruct-GGUF",
        "mradermacher/llama-3-meerkat-70b-v1.0-i1-GGUF",
        "user123/model_with_underscores-GGUF",
        "org-name/model-with-many-dashes-v2.0-GGUF",
    ];

    for repo_id in &valid_repo_ids {
        let model = create_downloaded_model(repo_id, "Q4_K_M");
        let result = database::add_model(&pool, &model).await;
        assert!(result.is_ok(), "Valid repo ID should succeed: {}", repo_id);
    }

    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), valid_repo_ids.len());

    // Verify all repo IDs are stored correctly
    for model in &models {
        assert!(model.hf_repo_id.is_some());
        let repo_id = model.hf_repo_id.as_ref().unwrap();
        assert!(
            repo_id.contains('/'),
            "Repo ID should contain namespace/model format"
        );
        assert!(valid_repo_ids.contains(&repo_id.as_str()));
    }
}
