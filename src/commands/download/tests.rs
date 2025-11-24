#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::tempdir;
    use crate::commands::download::file_ops::extract_quantization_from_filename;
    use crate::commands::download::utils::{get_models_directory, sanitize_model_name};
    use crate::commands::download::api::create_hf_api;

    #[test]
    fn test_extract_quantization_from_filename() {
        let test_cases = vec![
            ("model-Q4_K_M.gguf", "Q4_K_M"),
            ("llama-7b-Q8_0.gguf", "Q8_0"),
            ("model-f16.gguf", "F16"),
            ("model-F16.gguf", "F16"),
            ("model-fp16.gguf", "F16"),
            ("model-FP16.gguf", "F16"),
            ("model-Q4_0.gguf", "Q4_0"),
            ("model-q4_0.gguf", "Q4_0"),
            ("model-q6_k.gguf", "Q6_K"),
            ("model-Q6_K.gguf", "Q6_K"),
            ("model-q4.gguf", "Q4"),
            ("model-Q4.gguf", "Q4"),
            ("model-q6.gguf", "Q6"),
            ("model-Q6.gguf", "Q6"),
            ("model-q8.gguf", "Q8"),
            ("model-Q8.gguf", "Q8"),
            ("model-f32.gguf", "F32"),
            ("model-F32.gguf", "F32"),
            ("model-fp32.gguf", "F32"),
            ("model-FP32.gguf", "F32"),
            ("random-name.gguf", "unknown"),
            ("no-extension", "unknown"),
            ("", "unknown"),
        ];

        for (filename, expected) in test_cases {
            let result = extract_quantization_from_filename(filename);
            assert_eq!(result, expected, "Failed for filename: {}", filename);
        }
    }

    #[test]
    fn test_sanitize_model_name() {
        let test_cases = vec![
            ("microsoft/DialoGPT-medium", "microsoft_DialoGPT-medium"),
            ("meta-llama/Llama-2-7b-chat", "meta-llama_Llama-2-7b-chat"),
            ("model:with:colons", "model_with_colons"),
            ("path/with\\backslash", "path_with_backslash"),
            ("normal-model-name", "normal-model-name"),
            (
                "model/with\\multiple:separators",
                "model_with_multiple_separators",
            ),
            ("", ""),
            ("no-special-chars", "no-special-chars"),
        ];

        for (input, expected) in test_cases {
            let result = sanitize_model_name(input);
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_get_models_directory_respects_env_override() {
        let temp = tempdir().unwrap();
        let custom = temp.path().join("models");
        unsafe {
            env::set_var("GGLIB_MODELS_DIR", &custom);
        }

        let dir = get_models_directory().unwrap();
        assert_eq!(dir, custom);
        assert!(dir.exists());

        unsafe {
            env::remove_var("GGLIB_MODELS_DIR");
        }
    }

    #[test]
    fn test_quantization_detection_edge_cases() {
        // Test edge cases for quantization detection
        let edge_cases = vec![
            ("model-q4_k_m-extra.gguf", "Q4_K_M"), // Should match Q4_K_M even with extra text
            ("Q8_0-model.gguf", "Q8_0"),           // Quantization at start
            ("model-Q4_K_M-Q8_0.gguf", "Q4_K_M"), // Multiple patterns - should match first found (Q4_K_M comes first in logic)
            ("model-Mixed-q4_k_m.gguf", "Q4_K_M"), // Case insensitive
            ("f16-only.gguf", "F16"),           // Just the quantization type
            ("model-UNKNOWN.gguf", "unknown"),  // Unknown pattern
        ];

        for (filename, expected) in edge_cases {
            let result = extract_quantization_from_filename(filename);
            assert_eq!(
                result, expected,
                "Edge case failed for filename: {}",
                filename
            );
        }
    }

    #[test]
    fn test_model_name_sanitization_edge_cases() {
        // Test edge cases for model name sanitization
        let edge_cases = vec![
            ("///multiple///slashes", "___multiple___slashes"),
            (
                "\\\\\\multiple\\\\\\backslashes",
                "___multiple___backslashes",
            ),
            (":::multiple:::colons", "___multiple___colons"),
            ("/\\:/\\:/\\:", "_________"), // Each character gets replaced with underscore
            ("normal-text", "normal-text"), // Should remain unchanged
            ("under_scores_ok", "under_scores_ok"), // Underscores are fine
            ("dots.are.ok", "dots.are.ok"), // Dots are fine
            ("spaces are ok", "spaces are ok"), // Spaces are fine
        ];

        for (input, expected) in edge_cases {
            let result = sanitize_model_name(input);
            assert_eq!(result, expected, "Edge case failed for input: {}", input);
        }
    }

    #[tokio::test]
    async fn test_create_hf_api_without_token() {
        // Test that we can create an API client without a token
        // This will use the default cache directory logic

        let temp = tempdir().unwrap();
        let result = create_hf_api(None, temp.path());
        assert!(
            result.is_ok(),
            "Should be able to create API client without token"
        );
    }

    #[tokio::test]
    async fn test_create_hf_api_with_token() {
        // Test that we can create an API client with a token

        let temp = tempdir().unwrap();
        let result = create_hf_api(Some("fake_token".to_string()), temp.path());
        assert!(
            result.is_ok(),
            "Should be able to create API client with token"
        );
    }

    #[test]
    fn test_repo_id_parsing() {
        // Test parsing different repository ID formats
        let test_cases = vec![
            ("microsoft/DialoGPT-medium", "DialoGPT-medium"),
            ("meta-llama/Llama-2-7b-chat", "Llama-2-7b-chat"),
            ("single-name", "single-name"),
            ("org/model-GGUF", "model"), // Should strip -GGUF suffix
            ("complex/model-name-GGUF", "model-name"),
        ];

        for (repo_id, expected_model_name) in test_cases {
            let model_name = repo_id.split('/').next_back().unwrap_or("model");
            let clean_name = model_name.strip_suffix("-GGUF").unwrap_or(model_name);
            assert_eq!(
                clean_name, expected_model_name,
                "Failed for repo_id: {}",
                repo_id
            );
        }
    }

    // Note: Tests for functions that make network calls (like download_model,
    // list_available_quantizations) would require mocking the HuggingFace API,
    // which is complex and better handled in integration tests with test data.
    // The integration tests in tests/integration_download_command.rs handle
    // the database integration aspects of the download functionality.
}