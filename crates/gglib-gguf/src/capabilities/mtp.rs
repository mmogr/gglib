//! MTP (Multi-Token Prediction) capability detection.
//!
//! Detects whether a GGUF file contains embedded MTP draft heads by inspecting
//! the `{arch}.nextn_predict_layers` metadata key.
//!
//! # Detection Strategy
//!
//! MTP capability is determined **exclusively** from the GGUF key-value metadata.
//! The canonical key is `{arch}.nextn_predict_layers` (e.g.
//! `qwen3_5_mtp.nextn_predict_layers` for Qwen3.6 MTP).  A value strictly
//! greater than zero indicates that the file contains the bundled MTP head
//! tensors and is eligible for `--spec-type draft-mtp` speculative decoding.
//!
//! No filename or model-name heuristics are used: a model named `*-MTP` that
//! had its MTP heads stripped during quantisation would still not be tagged,
//! which prevents passing flags that would crash llama-server.

use std::collections::HashMap;

/// Detect MTP support from raw GGUF key-value metadata.
///
/// Scans all metadata keys for any key ending with `.nextn_predict_layers`.
/// If such a key is found and its value parses to a `u32` strictly greater
/// than zero, the model contains embedded MTP draft heads and MTP is
/// considered supported.
#[must_use]
pub(crate) fn detect_mtp_support(metadata: &HashMap<String, String>) -> bool {
    metadata.iter().any(|(key, value)| {
        key.ends_with(".nextn_predict_layers") && value.parse::<u32>().is_ok_and(|n| n > 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn detects_qwen_mtp_key() {
        let m = meta(&[("qwen3_5_mtp.nextn_predict_layers", "1")]);
        assert!(detect_mtp_support(&m));
    }

    #[test]
    fn detects_generic_arch_mtp_key() {
        let m = meta(&[("llama.nextn_predict_layers", "4")]);
        assert!(detect_mtp_support(&m));
    }

    #[test]
    fn absent_key_returns_not_supported() {
        let m = meta(&[
            ("llama.context_length", "4096"),
            ("general.name", "MyModel"),
        ]);
        assert!(!detect_mtp_support(&m));
    }

    #[test]
    fn zero_value_returns_not_supported() {
        let m = meta(&[("qwen3_5_mtp.nextn_predict_layers", "0")]);
        assert!(!detect_mtp_support(&m));
    }

    #[test]
    fn non_numeric_value_returns_not_supported() {
        let m = meta(&[("llama.nextn_predict_layers", "unknown")]);
        assert!(!detect_mtp_support(&m));
    }

    #[test]
    fn model_name_heuristic_does_not_trigger_detection() {
        // Ensure filename/name heuristics are NOT used — pure metadata key only.
        let m = meta(&[
            ("general.name", "Qwen3-27B-MTP"),
            ("llama.context_length", "32768"),
        ]);
        assert!(
            !detect_mtp_support(&m),
            "name heuristics must not trigger MTP detection"
        );
    }
}
