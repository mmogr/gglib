//! Tests for [`super::kv_memory_is_partial`] and [`super::kv_cache_layer_count`].
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use crate::domain::{KvElemsPerToken, estimate_kv_elems_per_token};

/// Qwen3.6-shaped metadata: hybrid attention, every 4th layer full.
fn qwen36_metadata() -> HashMap<String, String> {
    HashMap::from([
        ("general.architecture".to_string(), "qwen35".to_string()),
        (
            "qwen35.full_attention_interval".to_string(),
            "4".to_string(),
        ),
        ("qwen35.block_count".to_string(), "64".to_string()),
        ("qwen35.attention.head_count".to_string(), "24".to_string()),
    ])
}

// ── Partial-history detection ────────────────────────────────────────────────

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

// ── Cache-holding layer count ────────────────────────────────────────────────

/// The defect this function exists for: on a hybrid model only every
/// `full_attention_interval`-th layer keeps a KV cache, so Qwen3.8's 64 blocks
/// contribute 16. Counting all 64 budgets four times the KV the model needs.
#[test]
fn a_hybrid_interval_divides_the_block_count() {
    let count = kv_cache_layer_count(&qwen36_metadata(), Some("qwen35"));
    assert_eq!(count, Some(16));
}

/// The regression guard for every model gglib already serves: with no
/// `full_attention_interval` key every block holds a cache, so the count is
/// `block_count` untouched and the estimate is bit-identical to before.
#[test]
fn a_full_attention_model_counts_every_block() {
    let md = HashMap::from([
        ("general.architecture".to_string(), "qwen3".to_string()),
        ("qwen3.block_count".to_string(), "64".to_string()),
    ]);
    assert_eq!(kv_cache_layer_count(&md, Some("qwen3")), Some(64));
}

/// `full_attention_interval = 1` says every layer is full attention. Dividing
/// by it would be harmless arithmetic, but the `> 1` guard is what keeps the
/// key's meaning — a *marker* of hybridity — rather than a bare divisor.
#[test]
fn an_interval_of_one_is_not_a_divisor() {
    let mut md = qwen36_metadata();
    md.insert(
        "qwen35.full_attention_interval".to_string(),
        "1".to_string(),
    );
    assert_eq!(kv_cache_layer_count(&md, Some("qwen35")), Some(64));
}

/// When the interval does not divide the block count evenly the metadata
/// cannot say where the remainder falls, so the count rounds up: 65 blocks at
/// interval 4 is 17, not 16. One layer of over-estimate is the safe direction
/// for a memory budget; rounding down would promise memory that isn't there.
#[test]
fn an_uneven_interval_rounds_the_layer_count_up() {
    let mut md = qwen36_metadata();
    md.insert("qwen35.block_count".to_string(), "65".to_string());
    assert_eq!(kv_cache_layer_count(&md, Some("qwen35")), Some(17));
}

/// A garbage interval must not poison the count. Falling back to every block
/// over-states the budget, which is survivable; propagating `None` would
/// discard a `block_count` the file did carry.
#[test]
fn a_non_numeric_interval_is_ignored_rather_than_fatal() {
    let mut md = qwen36_metadata();
    md.insert(
        "qwen35.full_attention_interval".to_string(),
        "four".to_string(),
    );
    assert_eq!(kv_cache_layer_count(&md, Some("qwen35")), Some(64));
}

/// `block_count` has no fallback, and [`estimate_kv_elems_per_token`]
/// propagates its absence with `?`. Returning `None` keeps "we don't know"
/// travelling instead of collapsing into a zero that reads as "KV is free".
#[test]
fn a_missing_block_count_is_unknown_rather_than_zero() {
    let mut md = qwen36_metadata();
    md.remove("qwen35.block_count");
    assert_eq!(kv_cache_layer_count(&md, Some("qwen35")), None);
}

/// The seam, asserted through the public entry point rather than the helper:
/// a Qwen3.8-shaped GGUF header must reach the estimate as 16 cache-holding
/// layers, not 64. At f16 that is 64 KiB/token instead of 256 — 16 GiB rather
/// than 64 GiB at the model's 262144-token context, which is the difference
/// between a launch that fits and one the residency check refuses.
#[test]
fn the_estimate_counts_only_the_cache_holding_layers_of_a_hybrid_model() {
    let md = HashMap::from([
        ("general.architecture".to_string(), "qwen35".to_string()),
        ("qwen35.block_count".to_string(), "64".to_string()),
        (
            "qwen35.full_attention_interval".to_string(),
            "4".to_string(),
        ),
        ("qwen35.attention.head_count".to_string(), "32".to_string()),
        (
            "qwen35.attention.head_count_kv".to_string(),
            "4".to_string(),
        ),
        ("qwen35.attention.key_length".to_string(), "256".to_string()),
        (
            "qwen35.attention.value_length".to_string(),
            "256".to_string(),
        ),
    ]);
    // 16 cache-holding layers × 4 kv heads × 256 head dim.
    let expected = 16 * 4 * 256;
    assert_eq!(
        estimate_kv_elems_per_token(&md, Some("qwen35")),
        Some(KvElemsPerToken {
            k: expected,
            v: expected
        })
    );
}
