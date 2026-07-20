//! KV-cache size estimation from GGUF metadata.
//!
//! Estimates how many bytes of KV cache a model consumes *per token* of
//! context, which callers multiply by a context size to size memory budgets
//! (see `crate::server_config::compute_auto_cache_ram_mb`).
//!
//! Inputs come from the raw GGUF key/value map that `gglib-gguf` copies
//! verbatim into [`crate::domain::Model::metadata`], so no re-parse of the
//! `.gguf` file is needed. Every key is architecture-prefixed
//! (`qwen3.block_count`, `llama.attention.head_count_kv`, …).
//!
//! This is deliberately an *estimate*: it models the standard transformer
//! KV-cache layout and ignores architecture-specific extras (sliding-window
//! layers, MLA compression, per-layer overrides). It is used only for
//! conservative memory budgeting, never for correctness, and returns `None`
//! rather than guessing when the metadata doesn't carry what it needs.

use std::collections::HashMap;
use std::hash::BuildHasher;

/// Bytes per KV cache element. llama.cpp defaults both the K and V cache to
/// `f16` (2 bytes); quantized KV caches (`--cache-type-k q8_0` etc.) would use
/// less, so assuming `f16` keeps the estimate on the conservative (larger) side.
const BYTES_PER_KV_ELEMENT: u64 = 2;

/// Look up an architecture-prefixed GGUF key (`{arch}.{suffix}`), falling back
/// to the bare suffix for the occasional file that omits the prefix.
fn lookup<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    arch: &str,
    suffix: &str,
) -> Option<u64> {
    metadata
        .get(&format!("{arch}.{suffix}"))
        .or_else(|| metadata.get(suffix))
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// Estimate KV cache bytes consumed per token of context.
///
/// Formula (standard transformer KV cache):
///
/// ```text
/// bytes/token = block_count × head_count_kv × (key_length + value_length) × 2 bytes
/// ```
///
/// `key_length`/`value_length` are the per-head dimensions. When absent, both
/// fall back to `embedding_length / head_count` (the standard derivation).
/// `head_count_kv` falls back to `head_count` for models without grouped-query
/// attention.
///
/// # Arguments
///
/// * `metadata` — raw GGUF key/value map (see [`crate::domain::Model::metadata`]).
/// * `architecture` — the model's architecture, used as the key prefix. When
///   `None`, falls back to the `general.architecture` metadata key.
///
/// # Returns
///
/// `None` when the metadata lacks the layer/head counts needed to compute a
/// meaningful figure (or carries non-numeric values) — callers should treat
/// that as "unknown" and substitute their own conservative allowance rather
/// than assuming zero.
#[must_use]
pub fn estimate_kv_bytes_per_token<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    architecture: Option<&str>,
) -> Option<u64> {
    let arch = architecture
        .map(str::to_owned)
        .or_else(|| metadata.get("general.architecture").cloned())?;
    let arch = arch.trim().to_ascii_lowercase();

    let block_count = lookup(metadata, &arch, "block_count")?;
    let head_count = lookup(metadata, &arch, "attention.head_count");
    // Grouped-query attention shrinks the KV cache: prefer head_count_kv.
    let head_count_kv = lookup(metadata, &arch, "attention.head_count_kv").or(head_count)?;

    // Per-head K/V dimensions, explicit when present (some architectures use
    // asymmetric or non-derivable head dims), else derived from the hidden size.
    let derived_head_dim = || {
        let embedding_length = lookup(metadata, &arch, "embedding_length")?;
        let heads = head_count?;
        (heads > 0).then(|| embedding_length / heads)
    };
    let key_length = lookup(metadata, &arch, "attention.key_length").or_else(derived_head_dim)?;
    let value_length =
        lookup(metadata, &arch, "attention.value_length").or_else(derived_head_dim)?;

    if block_count == 0 || head_count_kv == 0 {
        return None;
    }

    Some(
        block_count
            .saturating_mul(head_count_kv)
            .saturating_mul(key_length.saturating_add(value_length))
            .saturating_mul(BYTES_PER_KV_ELEMENT),
    )
}

/// Estimate total KV cache bytes for a given context size.
///
/// Convenience wrapper over [`estimate_kv_bytes_per_token`]; saturating so an
/// absurd context size can never overflow into a small (and therefore
/// dangerously permissive) budget.
#[must_use]
pub const fn estimate_kv_bytes_for_context(kv_bytes_per_token: u64, context_size: u64) -> u64 {
    kv_bytes_per_token.saturating_mul(context_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Qwen3-shaped metadata with grouped-query attention and explicit head dims.
    fn qwen_metadata() -> HashMap<String, String> {
        HashMap::from([
            ("general.architecture".to_string(), "qwen3".to_string()),
            ("qwen3.block_count".to_string(), "64".to_string()),
            ("qwen3.attention.head_count".to_string(), "40".to_string()),
            ("qwen3.attention.head_count_kv".to_string(), "8".to_string()),
            ("qwen3.embedding_length".to_string(), "5120".to_string()),
            ("qwen3.attention.key_length".to_string(), "128".to_string()),
            (
                "qwen3.attention.value_length".to_string(),
                "128".to_string(),
            ),
        ])
    }

    #[test]
    fn computes_from_explicit_head_dims() {
        // 64 layers × 8 kv heads × (128 + 128) × 2 bytes = 262144 bytes/token
        let got = estimate_kv_bytes_per_token(&qwen_metadata(), Some("qwen3"));
        assert_eq!(got, Some(64 * 8 * (128 + 128) * 2));
    }

    #[test]
    fn architecture_falls_back_to_general_architecture_key() {
        // No explicit architecture passed — read it from the metadata itself.
        let got = estimate_kv_bytes_per_token(&qwen_metadata(), None);
        assert_eq!(got, Some(64 * 8 * (128 + 128) * 2));
    }

    #[test]
    fn architecture_lookup_is_case_insensitive() {
        let got = estimate_kv_bytes_per_token(&qwen_metadata(), Some("QWEN3"));
        assert_eq!(got, Some(64 * 8 * (128 + 128) * 2));
    }

    #[test]
    fn derives_head_dim_from_embedding_length_when_absent() {
        let mut md = qwen_metadata();
        md.remove("qwen3.attention.key_length");
        md.remove("qwen3.attention.value_length");
        // head_dim = 5120 / 40 = 128, so the result matches the explicit case.
        let got = estimate_kv_bytes_per_token(&md, Some("qwen3"));
        assert_eq!(got, Some(64 * 8 * (128 + 128) * 2));
    }

    /// Without GQA metadata the full head count is the KV head count — a much
    /// larger cache, which the estimate must reflect.
    #[test]
    fn falls_back_to_head_count_without_gqa() {
        let mut md = qwen_metadata();
        md.remove("qwen3.attention.head_count_kv");
        let got = estimate_kv_bytes_per_token(&md, Some("qwen3"));
        assert_eq!(got, Some(64 * 40 * (128 + 128) * 2));
    }

    #[test]
    fn none_when_block_count_missing() {
        let mut md = qwen_metadata();
        md.remove("qwen3.block_count");
        assert_eq!(estimate_kv_bytes_per_token(&md, Some("qwen3")), None);
    }

    #[test]
    fn none_when_head_counts_missing() {
        let mut md = qwen_metadata();
        md.remove("qwen3.attention.head_count");
        md.remove("qwen3.attention.head_count_kv");
        assert_eq!(estimate_kv_bytes_per_token(&md, Some("qwen3")), None);
    }

    /// Head dims are neither explicit nor derivable without `embedding_length`.
    #[test]
    fn none_when_head_dim_underivable() {
        let mut md = qwen_metadata();
        md.remove("qwen3.attention.key_length");
        md.remove("qwen3.attention.value_length");
        md.remove("qwen3.embedding_length");
        assert_eq!(estimate_kv_bytes_per_token(&md, Some("qwen3")), None);
    }

    #[test]
    fn none_on_non_numeric_values() {
        let mut md = qwen_metadata();
        md.insert("qwen3.block_count".to_string(), "sixty-four".to_string());
        assert_eq!(estimate_kv_bytes_per_token(&md, Some("qwen3")), None);
    }

    #[test]
    fn none_when_metadata_empty() {
        assert_eq!(
            estimate_kv_bytes_per_token(&HashMap::new(), Some("llama")),
            None
        );
        assert_eq!(estimate_kv_bytes_per_token(&HashMap::new(), None), None);
    }

    /// A zero layer/head count would produce a nonsense zero-byte estimate,
    /// which downstream would read as "KV is free" — reject it instead.
    #[test]
    fn none_on_degenerate_zero_counts() {
        let mut md = qwen_metadata();
        md.insert("qwen3.block_count".to_string(), "0".to_string());
        assert_eq!(estimate_kv_bytes_per_token(&md, Some("qwen3")), None);

        let mut md = qwen_metadata();
        md.insert("qwen3.attention.head_count_kv".to_string(), "0".to_string());
        assert_eq!(estimate_kv_bytes_per_token(&md, Some("qwen3")), None);
    }

    #[test]
    fn unprefixed_keys_are_accepted_as_a_fallback() {
        let md = HashMap::from([
            ("block_count".to_string(), "32".to_string()),
            ("attention.head_count".to_string(), "32".to_string()),
            ("attention.head_count_kv".to_string(), "8".to_string()),
            ("embedding_length".to_string(), "4096".to_string()),
        ]);
        // head_dim = 4096 / 32 = 128
        assert_eq!(
            estimate_kv_bytes_per_token(&md, Some("llama")),
            Some(32 * 8 * (128 + 128) * 2)
        );
    }

    #[test]
    fn context_multiplication_saturates() {
        assert_eq!(estimate_kv_bytes_for_context(1024, 100), 102_400);
        assert_eq!(estimate_kv_bytes_for_context(u64::MAX, u64::MAX), u64::MAX);
    }
}
