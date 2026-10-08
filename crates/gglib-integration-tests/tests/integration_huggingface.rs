//! How the file names a `HuggingFace` repository lists resolve to
//! quantizations: one group per quantization, and the shards of one file in
//! the same group.

use gglib_core::download::Quantization;

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
