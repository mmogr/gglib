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

/// Result of MTP capability detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtpDetection {
    /// Whether the model contains embedded MTP draft heads.
    pub supported: bool,
    /// Number of MTP prediction layers present (`nextn_predict_layers` value).
    /// Zero when `supported` is `false`.
    pub layer_count: u32,
}

/// Detect MTP support from raw GGUF key-value metadata.
///
/// Scans all metadata keys for any key ending with `.nextn_predict_layers`.
/// If such a key is found and its value parses to a `u32` strictly greater
/// than zero, MTP is considered supported.
///
/// Returns [`MtpDetection`] with `supported = false` and `layer_count = 0`
/// when no such key exists or the value is zero.
#[must_use]
pub fn detect_mtp_support(metadata: &HashMap<String, String>) -> MtpDetection {
    for (key, value) in metadata {
        if key.ends_with(".nextn_predict_layers") {
            if let Ok(n) = value.parse::<u32>() {
                if n > 0 {
                    return MtpDetection {
                        supported: true,
                        layer_count: n,
                    };
                }
            }
        }
    }

    MtpDetection {
        supported: false,
        layer_count: 0,
    }
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
        let det = detect_mtp_support(&m);
        assert!(det.supported);
        assert_eq!(det.layer_count, 1);
    }

    #[test]
    fn detects_generic_arch_mtp_key() {
        let m = meta(&[("llama.nextn_predict_layers", "4")]);
        let det = detect_mtp_support(&m);
        assert!(det.supported);
        assert_eq!(det.layer_count, 4);
    }

    #[test]
    fn absent_key_returns_not_supported() {
        let m = meta(&[
            ("llama.context_length", "4096"),
            ("general.name", "MyModel"),
        ]);
        let det = detect_mtp_support(&m);
        assert!(!det.supported);
        assert_eq!(det.layer_count, 0);
    }

    #[test]
    fn zero_value_returns_not_supported() {
        let m = meta(&[("qwen3_5_mtp.nextn_predict_layers", "0")]);
        let det = detect_mtp_support(&m);
        assert!(!det.supported);
        assert_eq!(det.layer_count, 0);
    }

    #[test]
    fn non_numeric_value_returns_not_supported() {
        let m = meta(&[("llama.nextn_predict_layers", "unknown")]);
        let det = detect_mtp_support(&m);
        assert!(!det.supported);
    }

    #[test]
    fn model_name_heuristic_does_not_trigger_detection() {
        // Ensure filename/name heuristics are NOT used — pure metadata key only.
        let m = meta(&[
            ("general.name", "Qwen3-27B-MTP"),
            ("llama.context_length", "32768"),
        ]);
        let det = detect_mtp_support(&m);
        assert!(
            !det.supported,
            "name heuristics must not trigger MTP detection"
        );
    }
}
