#[cfg(test)]
mod tests {
    use crate::commands::download::api::create_hf_api;
    use crate::commands::download::file_ops::{Quantization, extract_quantization_from_filename};
    use crate::commands::download::utils::{get_models_directory, sanitize_model_name};
    use std::env;
    use tempfile::tempdir;

    #[test]
    fn test_extract_quantization_from_filename() {
        let test_cases = vec![
            ("model-Q4_K_M.gguf", Quantization::Q4KM),
            ("llama-7b-Q8_0.gguf", Quantization::Q8_0),
            ("model-f16.gguf", Quantization::F16),
            ("model-F16.gguf", Quantization::F16),
            ("model-fp16.gguf", Quantization::F16),
            ("model-FP16.gguf", Quantization::F16),
            ("model-Q4_0.gguf", Quantization::Q4_0),
            ("model-q4_0.gguf", Quantization::Q4_0),
            ("model-q6_k.gguf", Quantization::Q6K),
            ("model-Q6_K.gguf", Quantization::Q6K),
            ("model-q4.gguf", Quantization::Q4),
            ("model-Q4.gguf", Quantization::Q4),
            ("model-q6.gguf", Quantization::Q6),
            ("model-Q6.gguf", Quantization::Q6),
            ("model-q8.gguf", Quantization::Q8),
            ("model-Q8.gguf", Quantization::Q8),
            ("model-f32.gguf", Quantization::F32),
            ("model-F32.gguf", Quantization::F32),
            ("model-fp32.gguf", Quantization::F32),
            ("model-FP32.gguf", Quantization::F32),
            ("random-name.gguf", Quantization::Unknown),
            ("no-extension", Quantization::Unknown),
            ("", Quantization::Unknown),
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
            ("model-q4_k_m-extra.gguf", Quantization::Q4KM), // Should match Q4_K_M even with extra text
            ("Q8_0-model.gguf", Quantization::Q8_0),         // Quantization at start
            ("model-Q4_K_M-Q8_0.gguf", Quantization::Q4KM), // Multiple patterns - should match first found (Q4_K_M comes first in pattern table)
            ("model-Mixed-q4_k_m.gguf", Quantization::Q4KM), // Case insensitive
            ("f16-only.gguf", Quantization::F16),           // Just the quantization type
            ("model-UNKNOWN.gguf", Quantization::Unknown),  // Unknown pattern
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
    fn test_quantization_prefers_specific_over_generic() {
        // Test that more specific patterns are matched before generic ones
        // This verifies the pattern table ordering is correct
        let specificity_cases = vec![
            // Q4_K_M should be matched before Q4
            ("model-Q4_K_M-and-Q4.gguf", Quantization::Q4KM),
            // Q6_K should be matched before Q6
            ("model-with-Q6_K-suffix.gguf", Quantization::Q6K),
            // IQ2_XXS should be matched before IQ2_XS
            ("model-IQ2_XXS-test.gguf", Quantization::Iq2Xxs),
        ];

        for (filename, expected) in specificity_cases {
            let result = extract_quantization_from_filename(filename);
            assert_eq!(
                result, expected,
                "Specificity test failed for filename: {} (got {:?})",
                filename, result
            );
        }
    }

    #[test]
    fn test_quantization_to_string() {
        // Test that the Display implementation works correctly
        assert_eq!(Quantization::Q4KM.to_string(), "Q4_K_M");
        assert_eq!(Quantization::F16.to_string(), "F16");
        assert_eq!(Quantization::Unknown.to_string(), "unknown");
        assert_eq!(Quantization::Iq2Xxs.to_string(), "IQ2_XXS");
    }

    #[test]
    fn test_quantization_is_unknown() {
        assert!(Quantization::Unknown.is_unknown());
        assert!(!Quantization::Q4KM.is_unknown());
        assert!(!Quantization::F16.is_unknown());
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
