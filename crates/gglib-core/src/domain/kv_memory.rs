//! Detection of partial-KV-memory architectures from GGUF metadata.
//!
//! Some model architectures do not retain the full token history in their KV
//! memory: sliding-window attention (SWA) layers keep only a recent window,
//! hybrid-attention models interleave SWA layers with full-attention layers,
//! and recurrent (SSM/Mamba-family) models keep only a compressed state.
//!
//! This matters for llama-server's disk slot persistence
//! (`/slots?action=save|restore`): the save path serializes only the sequence
//! KV state and token list — **not** the server's context checkpoints — and
//! the restore path clears the slot's checkpoint list. On a full-attention
//! model that's fine (the KV state alone is sufficient to resume). On a
//! partial-memory model, resuming from position `n_past` requires history the
//! SWA/recurrent layers no longer hold, which llama-server bridges with
//! context checkpoints; with the checkpoint list empty after a disk restore,
//! it falls back to `n_past = 0` and reprocesses the *entire* prompt. A disk
//! "restore" on such a model therefore costs a full re-prefill — worse than
//! useless, since the in-RAM prompt cache (`--cache-ram`), which *does* carry
//! checkpoints, would have resumed cheaply had the slot not been pre-filled
//! by the restore.
//!
//! Inputs come from the raw GGUF key/value map that `gglib-gguf` copies
//! verbatim into [`crate::domain::Model::metadata`] (see
//! [`crate::domain::estimate_kv_elems_per_token`] for the same pattern).
//!
//! Detection is deliberately *sensitive*: a false positive merely forgoes the
//! disk-cache layer (the in-RAM cache still works), while a false negative
//! silently costs minutes of TTFT per restore. Some older GGUFs carry a
//! `sliding_window` key the runtime ignores; treating them as partial is the
//! safe direction.

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Look up an architecture-prefixed GGUF key (`{arch}.{suffix}`), falling back
/// to the bare suffix for the occasional file that omits the prefix.
fn lookup_raw<'m, S: BuildHasher>(
    metadata: &'m HashMap<String, String, S>,
    arch: &str,
    suffix: &str,
) -> Option<&'m str> {
    metadata
        .get(&format!("{arch}.{suffix}"))
        .or_else(|| metadata.get(suffix))
        .map(|v| v.trim())
}

/// Numeric variant of [`lookup_raw`].
fn lookup_u64<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    arch: &str,
    suffix: &str,
) -> Option<u64> {
    lookup_raw(metadata, arch, suffix).and_then(|v| v.parse::<u64>().ok())
}

/// Whether the model's KV memory retains only part of the token history
/// (sliding-window, hybrid, or recurrent attention).
///
/// Checks, in order:
///
/// * `{arch}.full_attention_interval` > 1 — hybrid interleaved attention
///   (e.g. `qwen35.full_attention_interval = 4`: every 4th layer is full
///   attention, the rest sliding-window).
/// * `{arch}.attention.sliding_window` > 0 — sliding-window attention
///   (Gemma 2/3, Cohere 2, GPT-OSS, …).
/// * `{arch}.ssm.conv_kernel` present — recurrent / hybrid-recurrent state
///   (Mamba, Jamba, Granite-H, Falcon-H, …), which is inherently partial.
///
/// # Arguments
///
/// * `metadata` — raw GGUF key/value map (see [`crate::domain::Model::metadata`]).
/// * `architecture` — the model's architecture, used as the key prefix. When
///   `None`, falls back to the `general.architecture` metadata key.
///
/// # Returns
///
/// `false` when the metadata carries none of the marker keys — including when
/// the architecture can't be determined at all, since an unprefixed lookup
/// still runs and full-attention is the common case.
#[must_use]
pub fn kv_memory_is_partial<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    architecture: Option<&str>,
) -> bool {
    let arch = architecture
        .map(str::to_owned)
        .or_else(|| metadata.get("general.architecture").cloned())
        .unwrap_or_default();
    let arch = arch.trim().to_ascii_lowercase();

    if lookup_u64(metadata, &arch, "full_attention_interval").is_some_and(|v| v > 1) {
        return true;
    }
    if lookup_u64(metadata, &arch, "attention.sliding_window").is_some_and(|v| v > 0) {
        return true;
    }
    if lookup_raw(metadata, &arch, "ssm.conv_kernel").is_some() {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Qwen3.6-shaped metadata: hybrid attention, every 4th layer full.
    fn qwen36_metadata() -> HashMap<String, String> {
        HashMap::from([
            ("general.architecture".to_string(), "qwen35".to_string()),
            (
                "qwen35.full_attention_interval".to_string(),
                "4".to_string(),
            ),
            ("qwen35.attention.head_count".to_string(), "24".to_string()),
        ])
    }

    #[test]
    fn detects_hybrid_full_attention_interval() {
        assert!(kv_memory_is_partial(&qwen36_metadata(), Some("qwen35")));
    }

    #[test]
    fn architecture_falls_back_to_general_architecture_key() {
        assert!(kv_memory_is_partial(&qwen36_metadata(), None));
    }

    #[test]
    fn architecture_lookup_is_case_insensitive() {
        assert!(kv_memory_is_partial(&qwen36_metadata(), Some("QWEN35")));
    }

    /// Interval of 1 means every layer is full attention — not partial.
    #[test]
    fn interval_of_one_is_full_attention() {
        let mut md = qwen36_metadata();
        md.insert(
            "qwen35.full_attention_interval".to_string(),
            "1".to_string(),
        );
        assert!(!kv_memory_is_partial(&md, Some("qwen35")));
    }

    #[test]
    fn detects_sliding_window_attention() {
        let md = HashMap::from([
            ("general.architecture".to_string(), "gemma3".to_string()),
            (
                "gemma3.attention.sliding_window".to_string(),
                "1024".to_string(),
            ),
        ]);
        assert!(kv_memory_is_partial(&md, Some("gemma3")));
    }

    /// A zero-size window means SWA is effectively disabled.
    #[test]
    fn zero_sliding_window_is_full_attention() {
        let md = HashMap::from([(
            "gemma3.attention.sliding_window".to_string(),
            "0".to_string(),
        )]);
        assert!(!kv_memory_is_partial(&md, Some("gemma3")));
    }

    #[test]
    fn detects_recurrent_ssm_state() {
        let md = HashMap::from([
            ("general.architecture".to_string(), "mamba".to_string()),
            ("mamba.ssm.conv_kernel".to_string(), "4".to_string()),
        ]);
        assert!(kv_memory_is_partial(&md, Some("mamba")));
    }

    /// Plain full-attention transformer metadata (the Qwen3 fixture shape
    /// from `kv_estimate`) must not trip the detector.
    #[test]
    fn full_attention_model_is_not_partial() {
        let md = HashMap::from([
            ("general.architecture".to_string(), "qwen3".to_string()),
            ("qwen3.block_count".to_string(), "64".to_string()),
            ("qwen3.attention.head_count".to_string(), "40".to_string()),
            ("qwen3.attention.head_count_kv".to_string(), "8".to_string()),
        ]);
        assert!(!kv_memory_is_partial(&md, Some("qwen3")));
    }

    #[test]
    fn empty_metadata_is_not_partial() {
        assert!(!kv_memory_is_partial(&HashMap::new(), Some("llama")));
        assert!(!kv_memory_is_partial(&HashMap::new(), None));
    }

    #[test]
    fn unprefixed_keys_are_accepted_as_a_fallback() {
        let md = HashMap::from([("attention.sliding_window".to_string(), "512".to_string())]);
        assert!(kv_memory_is_partial(&md, Some("gemma2")));
    }

    #[test]
    fn non_numeric_marker_values_are_ignored() {
        let md = HashMap::from([(
            "qwen35.full_attention_interval".to_string(),
            "four".to_string(),
        )]);
        assert!(!kv_memory_is_partial(&md, Some("qwen35")));
    }
}
