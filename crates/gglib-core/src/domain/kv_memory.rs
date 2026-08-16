//! The shape of a model's KV memory, read from GGUF metadata: whether it
//! retains the full token history, and how many layers hold a cache at all.
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
//!
//! [`kv_cache_layer_count`] answers the quantitative half of the same
//! question — how many layers hold a per-token cache — and is what
//! [`crate::domain::estimate_kv_elems_per_token`] sizes its budget from.

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

/// Resolve the GGUF key prefix: the caller's architecture when it knows one,
/// else the file's own `general.architecture`, normalised for lookup. Empty
/// when neither is available, which is harmless — [`lookup_raw`]'s unprefixed
/// fallback still runs.
fn resolve_arch<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    architecture: Option<&str>,
) -> String {
    architecture
        .map(str::to_owned)
        .or_else(|| metadata.get("general.architecture").cloned())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
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
    let arch = resolve_arch(metadata, architecture);

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

/// How many of the model's layers hold a per-token KV cache.
///
/// On a plain transformer that is every layer, so this is `{arch}.block_count`
/// unchanged. Hybrid-attention architectures interleave two kinds of layer:
/// `{arch}.full_attention_interval = 4` means every 4th layer is full
/// attention, so of Qwen3.8's 64 blocks only 16 keep a KV cache.
///
/// The other 48 are linear/SSM layers, and they contribute **zero** here on
/// purpose. Their state is a fixed-size summary — constant in context length,
/// not proportional to it — so its cost belongs in a weights-side allowance,
/// not in a per-token figure. A per-token figure is a slope: whatever it
/// carries gets multiplied by the context size. Folding those layers in
/// therefore over-counts them by the entire context — 256 KiB/token instead of
/// 64, i.e. 64 GiB rather than 16 GiB at Qwen3.8's 262144-token context.
///
/// Division rounds **up**. When the interval does not divide the block count
/// evenly the metadata alone cannot say which side the remainder falls on, and
/// counting one layer too many over-states the budget — the safe direction for
/// a figure the launcher plans memory against.
///
/// # Arguments
///
/// * `metadata` — raw GGUF key/value map (see [`crate::domain::Model::metadata`]).
/// * `architecture` — the model's architecture, used as the key prefix. When
///   `None`, falls back to the `general.architecture` metadata key.
///
/// # Returns
///
/// `None` when `block_count` is absent or non-numeric, so the "we don't know"
/// signal keeps travelling rather than collapsing into a zero that would read
/// as "KV is free" — [`crate::domain::estimate_kv_elems_per_token`] propagates
/// it with `?`. An absent, non-numeric, or `1` interval leaves `block_count`
/// untouched, so every full-attention model is bit-identical to before this
/// function existed.
#[must_use]
pub fn kv_cache_layer_count<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    architecture: Option<&str>,
) -> Option<u64> {
    let arch = resolve_arch(metadata, architecture);
    let block_count = lookup_u64(metadata, &arch, "block_count")?;

    // An interval of 1 (or none at all) means every layer is full attention,
    // so there is nothing to divide out.
    match lookup_u64(metadata, &arch, "full_attention_interval") {
        Some(interval) if interval > 1 => Some(block_count.div_ceil(interval)),
        _ => Some(block_count),
    }
}

#[cfg(test)]
#[path = "kv_memory_tests.rs"]
mod kv_memory_tests;
