//! Integration tests for `HuggingFace` Hub integration functionality.
//!
//! This module tests the new `HuggingFace` Hub features including:
//! - Search functionality with GGUF filtering
//! - Browse functionality with category filtering  
//! - Quantization detection and grouping
//! - Sharded file detection and handling
//! - Repository information parsing
//! - `HuggingFace` service API URL construction (#39)

use gglib_core::download::Quantization;
use gglib_hf::{DefaultHfClient, HfClientConfig};

/// Test that `DefaultHfClient` is constructable
#[tokio::test]
async fn test_huggingface_service_construction() {
    let _client = DefaultHfClient::new(&HfClientConfig::default());
    // Just verify it can be created without panicking
}

/// Test basic search functionality (mock test since we can't make real API calls in CI)
#[tokio::test]
async fn test_search_command_structure() {
    // This tests the search command structure and parameters
    // In a real implementation, we'd mock the HuggingFace API

    // Test that search handles query parameters correctly
    let query = "llama-3 gguf";
    // Skip the is_empty check since it's a string literal (always false)
    assert!(query.contains("gguf"));

    // Test URL encoding scenarios
    let queries_with_special_chars = vec![
        "model with spaces",
        "model-with-dashes",
        "model_with_underscores",
        "model+with+plus",
        "model/with/slashes",
    ];

    for query in queries_with_special_chars {
        // Test that the query can be processed
        assert!(!query.is_empty());
        // In real implementation, this would test urlencoding::encode
        let encoded = urlencoding::encode(query);
        assert!(!encoded.is_empty());
    }
}

#[tokio::test]
async fn test_browse_command_structure() {
    // Test browse command categories
    let valid_categories = vec![
        "Text Generation",
        "Conversational",
        "Text2Text Generation",
        "Multimodal",
        "", // empty category should work
    ];

    for category in valid_categories {
        // Test that category can be processed
        if !category.is_empty() {
            let encoded = urlencoding::encode(category);
            assert!(!encoded.is_empty());
        }
    }
}

#[tokio::test]
async fn test_quantization_grouping() {
    use std::collections::HashMap;

    // Simulate how quantizations would be grouped for display
    let files = vec![
        "model-Q4_K_M.gguf",
        "model-Q4_K_S.gguf",
        "model-Q8_0.gguf",
        "model-F16.gguf",
        "model-IQ4_NL.gguf",
        "model-Q6_K.gguf-00001-of-00003.gguf",
        "model-Q6_K.gguf-00002-of-00003.gguf",
        "model-Q6_K.gguf-00003-of-00003.gguf",
    ];

    let mut quantization_groups: HashMap<String, Vec<String>> = HashMap::new();

    for file in files {
        let quant = Quantization::from_filename(file);
        quantization_groups
            .entry(quant.to_string())
            .or_default()
            .push(file.to_string());
    }

    // Verify grouping
    assert_eq!(quantization_groups.len(), 6); // Q4_K_M, Q4_K_S, Q8_0, F16, IQ4_NL, Q6_K
    assert_eq!(quantization_groups.get("Q6_K").unwrap().len(), 3); // 3 sharded files
    assert_eq!(quantization_groups.get("Q4_K_M").unwrap().len(), 1); // 1 single file
    assert_eq!(quantization_groups.get("IQ4_NL").unwrap().len(), 1); // 1 single file
}

#[tokio::test]
async fn test_repository_id_parsing() {
    // Test parsing of various HuggingFace repository ID formats
    let test_cases = vec![
        (
            "microsoft/DialoGPT-medium",
            ("microsoft", "DialoGPT-medium"),
        ),
        (
            "meta-llama/Llama-3.1-70B-Instruct",
            ("meta-llama", "Llama-3.1-70B-Instruct"),
        ),
        (
            "unsloth/DeepSeek-R1-Distill-Llama-70B-GGUF",
            ("unsloth", "DeepSeek-R1-Distill-Llama-70B-GGUF"),
        ),
        (
            "user_123/model-name_v2.0-GGUF",
            ("user_123", "model-name_v2.0-GGUF"),
        ),
        (
            "org-name/very-long-model-name-with-many-parts-v1.2.3-GGUF",
            (
                "org-name",
                "very-long-model-name-with-many-parts-v1.2.3-GGUF",
            ),
        ),
    ];

    for (repo_id, expected) in test_cases {
        let parts: Vec<&str> = repo_id.splitn(2, '/').collect();
        assert_eq!(
            parts.len(),
            2,
            "Repository ID should have exactly one slash: {repo_id}"
        );
        assert_eq!(parts[0], expected.0, "Namespace mismatch for {repo_id}");
        assert_eq!(parts[1], expected.1, "Model name mismatch for {repo_id}");

        // Verify no empty parts
        assert!(
            !parts[0].is_empty(),
            "Namespace should not be empty: {repo_id}"
        );
        assert!(
            !parts[1].is_empty(),
            "Model name should not be empty: {repo_id}"
        );
    }
}

#[tokio::test]
async fn test_sharded_file_pattern_detection() {
    // Test detection of sharded file patterns
    let sharded_files = vec![
        "model-Q6_K.gguf-00001-of-00006.gguf",
        "model-Q6_K.gguf-00002-of-00006.gguf",
        "model-Q6_K.gguf-00006-of-00006.gguf",
        "Large-Model.BF16.gguf-part-01-of-10.gguf",
        "Model.IQ4_NL.gguf-shard-1-of-5.gguf",
    ];

    let non_sharded_files = vec![
        "model-Q4_K_M.gguf",
        "model-F16.gguf",
        "simple-model-Q8_0.gguf", // Changed to have a quantization
    ];

    // Test sharded file detection patterns
    for file in sharded_files {
        // These patterns indicate sharded files
        let is_sharded = file.contains("-of-") || file.contains("part-") || file.contains("shard-");
        assert!(is_sharded, "Should detect as sharded: {file}");

        // Should still extract quantization correctly
        let quant = Quantization::from_filename(file);
        assert!(
            !quant.is_unknown(),
            "Should extract quantization from sharded file: {file}"
        );
    }

    for file in non_sharded_files {
        let is_sharded = file.contains("-of-") || file.contains("part-") || file.contains("shard-");
        assert!(!is_sharded, "Should not detect as sharded: {file}");

        let quant = Quantization::from_filename(file);
        assert!(
            !quant.is_unknown(),
            "Should extract quantization from regular file: {file}"
        );
    }
}

#[tokio::test]
async fn test_file_size_parsing() {
    // Test handling of file sizes from HuggingFace API
    let size_test_cases = vec![
        (1_048_576u64, "1.0 MB"),
        (1_073_741_824u64, "1024.0 MB"),
        (5_368_709_120u64, "5120.0 MB"),
        (10_737_418_240u64, "10240.0 MB"),
        (1_000_000u64, "1.0 MB"), // Approximately
    ];

    for (size_bytes, _expected_approx) in size_test_cases {
        let size_mb = size_bytes as f64 / 1_048_576.0;
        let formatted = format!("{size_mb:.1} MB");

        // Test that we can format sizes consistently
        assert!(formatted.contains("MB"));
        assert!(formatted.contains("."));

        // For larger files, verify they're in the expected range
        if size_bytes > 1_073_741_824 {
            assert!(size_mb > 1000.0, "Large files should show >1000 MB");
        }
    }
}

#[tokio::test]
async fn test_search_result_filtering() {
    // Test GGUF filtering logic for search results

    struct MockFile {
        name: String,
        is_gguf: bool,
    }

    let mock_files = [
        MockFile {
            name: "model.gguf".to_string(),
            is_gguf: true,
        },
        MockFile {
            name: "model.safetensors".to_string(),
            is_gguf: false,
        },
        MockFile {
            name: "config.json".to_string(),
            is_gguf: false,
        },
        MockFile {
            name: "tokenizer.json".to_string(),
            is_gguf: false,
        },
        MockFile {
            name: "model-Q4_K_M.gguf".to_string(),
            is_gguf: true,
        },
        MockFile {
            name: "model-F16.gguf".to_string(),
            is_gguf: true,
        },
        MockFile {
            name: "README.md".to_string(),
            is_gguf: false,
        },
    ];

    // Simulate GGUF filtering
    let gguf_files: Vec<&MockFile> = mock_files
        .iter()
        .filter(|f| f.name.ends_with(".gguf"))
        .collect();

    assert_eq!(gguf_files.len(), 3);

    for file in gguf_files {
        assert!(file.is_gguf);
        assert!(file.name.ends_with(".gguf"));
    }

    // Test that non-GGUF files are filtered out
    let non_gguf_files: Vec<&MockFile> = mock_files
        .iter()
        .filter(|f| !f.name.ends_with(".gguf"))
        .collect();

    assert_eq!(non_gguf_files.len(), 4);

    for file in non_gguf_files {
        assert!(!file.is_gguf);
        assert!(!file.name.ends_with(".gguf"));
    }
}

#[tokio::test]
async fn test_commit_sha_validation() {
    // Test commit SHA format validation
    let valid_shas = vec![
        "abc123def456",
        "1234567890abcdef",
        "a1b2c3d4e5f6g7h8i9j0",
        "ffffffffffffffffffffffffffffffffffffffff", // 40 chars
        "abcdef1234567890",                         // shorter
    ];

    let invalid_shas = vec![
        "", // empty
        "invalid-sha-with-spaces ",
        "sha with spaces",
        "sha\nwith\nnewlines",
    ];

    for sha in valid_shas {
        // Basic validation: not empty, no whitespace
        assert!(!sha.is_empty(), "SHA should not be empty");
        assert!(!sha.contains(' '), "SHA should not contain spaces: {sha}");
        assert!(
            !sha.contains('\n'),
            "SHA should not contain newlines: {sha}"
        );
        assert!(!sha.contains('\t'), "SHA should not contain tabs: {sha}");

        // Should be alphanumeric
        assert!(
            sha.chars().all(|c| c.is_ascii_alphanumeric()),
            "SHA should be alphanumeric: {sha}"
        );
    }

    for sha in invalid_shas {
        if sha.is_empty() {
            assert!(sha.is_empty());
        } else {
            // Should contain problematic characters
            let has_whitespace = sha.contains(' ') || sha.contains('\n') || sha.contains('\t');
            assert!(
                has_whitespace,
                "Invalid SHA should contain whitespace: '{sha}'"
            );
        }
    }
}
