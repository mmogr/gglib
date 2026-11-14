//! KV-cache size estimation from GGUF metadata.
//!
//! Estimates the *elements* of KV cache a model holds ([`KvShape`]): a part
//! every token of context adds to, and, on a model whose file says which of
//! its layers slide, the sliding-window layers' part, which llama-server
//! holds at a fixed number of cells. Element counts are type-agnostic
//! (derived purely from model architecture); converting to bytes happens
//! separately, once the launch's resolved K/V cache types are known
//! (`--cache-type-k`/`--cache-type-v` may quantize K and V differently — see
//! [`crate::cache_config::KvCacheType`]). [`KvBytes::at`] is the one place
//! the bytes at a context are computed.
//!
//! Inputs come from the raw GGUF key/value map that `gglib-gguf` copies
//! into [`crate::domain::Model::metadata`], so no re-parse of the `.gguf`
//! file is needed. Every key is architecture-prefixed
//! (`qwen3.block_count`, `llama.attention.head_count_kv`, …), and a
//! per-layer list is stored as JSON array text (`[16, 16, 4]`).
//!
//! This is deliberately an *estimate*: on a file that says which layers
//! slide it counts every block, else only the layers holding a cache (see
//! [`crate::domain::kv_memory::kv_cache_layer_count`]). It models no other
//! extras (layers sharing a cache, MLA compression, an MTP draft's own
//! context). It is for conservative memory budgeting, never for correctness.

use std::collections::HashMap;
use std::hash::BuildHasher;

use serde::de::DeserializeOwned;

use crate::cache_config::KvCacheType;
use crate::domain::kv_memory::kv_cache_layer_count;

/// The micro-batch llama-server adds to the window: its default `n_ubatch`.
const UBATCH: u64 = 512;

/// The cell count llama.cpp pads a sliding-window cache to.
const WINDOW_PAD: u64 = 256;

/// Per-token K and V element counts, type-agnostic.
///
/// K and V element counts are tracked separately (not just summed) because
/// callers may quantize K and V to different types (e.g. `q8_0` K with `f16`
/// V to sidestep the Flash Attention requirement on quantized V), so the byte
/// cost of each side must be computed independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvElemsPerToken {
    pub k: u64,
    pub v: u64,
}

/// The sliding-window layers' cache: `per_cell` for each cell of a block
/// sized by a window of `tokens`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlidingWindow<T> {
    /// The window, `{arch}.attention.sliding_window`, in tokens.
    pub tokens: u64,
    /// What each cell costs, over every sliding layer.
    pub per_cell: T,
}

/// The KV cache a model holds, type-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvShape {
    /// What each token of context costs, over the layers that keep every
    /// token.
    pub per_token: KvElemsPerToken,
    /// The sliding-window layers, when the file says which layers slide.
    pub window: Option<SlidingWindow<KvElemsPerToken>>,
}

/// [`KvShape`] in bytes, at a launch's K and V cache types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvBytes {
    /// Bytes each token of context costs, over the layers that keep every
    /// token.
    pub per_token: u64,
    /// The sliding-window layers, in bytes a cell.
    pub window: Option<SlidingWindow<u64>>,
}

impl KvShape {
    /// A shape whose every cached layer keeps every token.
    #[must_use]
    pub const fn uniform(per_token: KvElemsPerToken) -> Self {
        Self {
            per_token,
            window: None,
        }
    }

    /// This shape as a context made with llama.cpp's default parameters
    /// holds it: `swa_full` keeps every sliding layer at the full context,
    /// so each costs what it would a token.
    #[must_use]
    pub const fn full_size_window(self) -> Self {
        match self.window {
            Some(window) => Self::uniform(KvElemsPerToken {
                k: self.per_token.k.saturating_add(window.per_cell.k),
                v: self.per_token.v.saturating_add(window.per_cell.v),
            }),
            None => self,
        }
    }

    /// The shape in bytes, at `k` and `v`.
    #[must_use]
    pub const fn bytes(self, k: KvCacheType, v: KvCacheType) -> KvBytes {
        let window = match self.window {
            Some(window) => Some(SlidingWindow {
                tokens: window.tokens,
                per_cell: elems_bytes(window.per_cell, k, v),
            }),
            None => None,
        };
        KvBytes {
            per_token: elems_bytes(self.per_token, k, v),
            window,
        }
    }
}

/// Bytes of `elems`, K at `k` and V at `v`.
const fn elems_bytes(elems: KvElemsPerToken, k: KvCacheType, v: KvCacheType) -> u64 {
    k.bytes_for_elems(elems.k)
        .saturating_add(v.bytes_for_elems(elems.v))
}

impl KvBytes {
    /// A cache of `per_token` bytes a token and no sliding window.
    #[must_use]
    pub const fn uniform(per_token: u64) -> Self {
        Self {
            per_token,
            window: None,
        }
    }

    /// The KV cache at a context of `ctx` tokens.
    ///
    /// The layers that keep every token cost `per_token` each token. The
    /// sliding layers hold the block llama-server makes them
    /// (`llama_kv_cache_iswa`): the window and a micro-batch, no more than
    /// the context, padded to 256 cells. Saturating, so an absurd context
    /// can never overflow into a small (and therefore dangerously
    /// permissive) budget.
    #[must_use]
    pub const fn at(self, ctx: u64) -> u64 {
        let window = match self.window {
            Some(window) => {
                let span = window.tokens.saturating_add(UBATCH);
                let cells = if ctx < span { ctx } else { span };
                window
                    .per_cell
                    .saturating_mul(cells.next_multiple_of(WINDOW_PAD))
            }
            None => 0,
        };
        self.per_token.saturating_mul(ctx).saturating_add(window)
    }

    /// The largest context whose cache ([`Self::at`]) fits in `bytes`.
    ///
    /// A search over [`Self::at`] rather than a division, so it is exact
    /// whatever the window: `at` never falls as the context grows.
    #[must_use]
    pub const fn context_within(self, bytes: u64) -> u64 {
        let (mut fits, mut over) = (0, u64::MAX);
        if self.at(over) <= bytes {
            return over;
        }
        while over - fits > 1 {
            let mid = fits + (over - fits) / 2;
            if self.at(mid) <= bytes {
                fits = mid;
            } else {
                over = mid;
            }
        }
        fits
    }
}

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
fn lookup<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    arch: &str,
    suffix: &str,
) -> Option<u64> {
    lookup_raw(metadata, arch, suffix).and_then(|v| v.parse::<u64>().ok())
}

/// A key's list, one value a layer, when it holds one of `layers` values.
/// `None` when absent, a scalar, unreadable (a header stored before lists
/// were kept whole holds `[Array with N elements]`), or of another length.
fn layer_list<T: DeserializeOwned, S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    arch: &str,
    suffix: &str,
    layers: u64,
) -> Option<Vec<T>> {
    let list: Vec<T> = serde_json::from_str(lookup_raw(metadata, arch, suffix)?).ok()?;
    (u64::try_from(list.len()) == Ok(layers)).then_some(list)
}

/// Each layer's KV heads: a list of one a layer, or one count for all.
enum Heads {
    Each(Vec<u64>),
    All(u64),
}

impl Heads {
    fn of(&self, layer: usize) -> u64 {
        match self {
            Self::Each(heads) => heads[layer],
            Self::All(heads) => *heads,
        }
    }

    fn most(&self) -> Option<u64> {
        match self {
            Self::Each(heads) => heads.iter().copied().max(),
            Self::All(heads) => Some(*heads),
        }
    }
}

/// Estimate the KV cache the model holds.
///
/// Layer by layer, when the file says which layers slide
/// (`{arch}.attention.sliding_window_pattern`, a list of bools or a period)
/// and how wide their window is (`{arch}.attention.sliding_window`):
///
/// ```text
/// per token = Σ full layers    head_count_kv[i] × key_length     (and value_length)
/// per cell  = Σ sliding layers head_count_kv[i] × key_length_swa (and value_length_swa)
/// ```
///
/// `head_count_kv` is a list or a scalar, and `key_length_swa` and
/// `value_length_swa` fall back to `key_length` and `value_length`. A file
/// whose lists cannot be read, or have a value for some other number of
/// layers, is counted as one with no pattern.
///
/// Otherwise every layer that holds a cache ([`kv_cache_layer_count`]) is
/// counted per token, at the most KV heads any layer has:
///
/// ```text
/// k_elems/token = kv_cache_layers × head_count_kv × key_length
/// v_elems/token = kv_cache_layers × head_count_kv × value_length
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
pub fn estimate_kv_shape<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    architecture: Option<&str>,
) -> Option<KvShape> {
    let arch = architecture
        .map(str::to_owned)
        .or_else(|| metadata.get("general.architecture").cloned())?;
    let arch = arch.trim().to_ascii_lowercase();

    let head_count = lookup(metadata, &arch, "attention.head_count");
    // Per-head K/V dimensions, explicit when present (some architectures use
    // asymmetric or non-derivable head dims), else derived from the hidden size.
    let derived_head_dim = || {
        let embedding_length = lookup(metadata, &arch, "embedding_length")?;
        let heads = head_count?;
        (heads > 0).then(|| embedding_length / heads)
    };
    let full = KvElemsPerToken {
        k: lookup(metadata, &arch, "attention.key_length").or_else(derived_head_dim)?,
        v: lookup(metadata, &arch, "attention.value_length").or_else(derived_head_dim)?,
    };

    if let Some(shape) = per_layer_shape(metadata, &arch, head_count, full) {
        return Some(shape);
    }

    let cache_layers = kv_cache_layer_count(metadata, Some(&arch))?;
    let block_count = lookup(metadata, &arch, "block_count")?;
    // Grouped-query attention shrinks the KV cache: prefer head_count_kv.
    let head_count_kv = lookup(metadata, &arch, "attention.head_count_kv")
        .or_else(|| {
            layer_list::<u64, S>(metadata, &arch, "attention.head_count_kv", block_count)?
                .into_iter()
                .max()
        })
        .or(head_count)?;

    if cache_layers == 0 || head_count_kv == 0 {
        return None;
    }

    Some(KvShape::uniform(heads_times(
        cache_layers.saturating_mul(head_count_kv),
        full,
    )))
}

/// `heads` KV heads of `len` per-head K and V lengths.
const fn heads_times(heads: u64, len: KvElemsPerToken) -> KvElemsPerToken {
    KvElemsPerToken {
        k: heads.saturating_mul(len.k),
        v: heads.saturating_mul(len.v),
    }
}

/// The shape counted layer by layer, when the file says which layers slide
/// and how wide their window is, and each per-layer list it gives holds a
/// value for every layer. `full` is a full layer's per-head K and V length.
fn per_layer_shape<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    arch: &str,
    head_count: Option<u64>,
    full: KvElemsPerToken,
) -> Option<KvShape> {
    let tokens = lookup(metadata, arch, "attention.sliding_window").filter(|&n| n > 0)?;
    let layers = lookup(metadata, arch, "block_count")?;
    let key = "attention.head_count_kv";
    let heads = match (lookup_raw(metadata, arch, key), lookup(metadata, arch, key)) {
        (None, _) => Heads::All(head_count?),
        (Some(_), Some(heads)) => Heads::All(heads),
        (Some(_), None) => Heads::Each(layer_list(metadata, arch, key, layers)?),
    };
    let (full_heads, sliding_heads) = split_heads(metadata, arch, layers, &heads)?;
    let sliding = KvElemsPerToken {
        k: lookup(metadata, arch, "attention.key_length_swa").unwrap_or(full.k),
        v: lookup(metadata, arch, "attention.value_length_swa").unwrap_or(full.v),
    };
    let window = (sliding_heads > 0).then(|| SlidingWindow {
        tokens,
        per_cell: heads_times(sliding_heads, sliding),
    });
    (full_heads > 0 || window.is_some()).then(|| KvShape {
        per_token: heads_times(full_heads, full),
        window,
    })
}

/// The KV heads of the full layers and of the sliding layers, by
/// `{arch}.attention.sliding_window_pattern`: a list of bools, one a layer,
/// or a period `n`, which makes one layer in `n` full (none when `n` is 0).
/// The file does not say which layers a period makes full, so then each
/// layer counts at the most KV heads any has, and the full ones round up.
fn split_heads<S: BuildHasher>(
    metadata: &HashMap<String, String, S>,
    arch: &str,
    layers: u64,
    heads: &Heads,
) -> Option<(u64, u64)> {
    let suffix = "attention.sliding_window_pattern";
    if let Some(pattern) = layer_list::<bool, S>(metadata, arch, suffix, layers) {
        let (mut full, mut sliding) = (0_u64, 0_u64);
        for (layer, slides) in pattern.into_iter().enumerate() {
            let sum = if slides { &mut sliding } else { &mut full };
            *sum = sum.saturating_add(heads.of(layer));
        }
        return Some((full, sliding));
    }
    let period = lookup(metadata, arch, suffix)?;
    let full = if period == 0 {
        0
    } else {
        layers.div_ceil(period)
    };
    let most = heads.most()?;
    Some((
        full.saturating_mul(most),
        (layers - full).saturating_mul(most),
    ))
}

#[cfg(test)]
#[path = "kv_estimate_tests.rs"]
mod tests;
