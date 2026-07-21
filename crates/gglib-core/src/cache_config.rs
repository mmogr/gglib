//! KV-cache configuration types: quantized cache types and the host-RAM
//! prompt cache setting.
//!
//! Kept as a standalone, low-complexity module (not folded into
//! `server_config`) so cache-related config resolution has one home.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

// =============================================================================
// Host-RAM prompt cache budget setting (`--cache-ram`)
// =============================================================================

/// How to determine the host-RAM prompt cache budget (`--cache-ram`).
///
/// Deliberately a two-state enum rather than `Option<u64>`: benchmark
/// launches (which must never gain a prompt cache — it would perturb
/// throughput measurements and RAM footprint) pass `ExplicitMb(0)`, which
/// unambiguously disables the cache rather than leaving it to an implicit
/// "no value" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheRamSetting {
    /// Compute a budget from system RAM, model size, and the KV estimate.
    /// The default variant — every launch surface auto-sizes unless it
    /// opts out.
    #[default]
    Auto,
    /// Use exactly this MiB value. `0` disables the cache.
    ExplicitMb(u64),
}

// =============================================================================
// KV cache quantization type (`--cache-type-k` / `--cache-type-v`)
// =============================================================================

/// A llama.cpp KV-cache element type, as accepted by `--cache-type-k` /
/// `--cache-type-v`.
///
/// Quantized K types are supported unconditionally by llama.cpp; quantized V
/// types additionally require Flash Attention to be active (llama.cpp
/// hard-errors at startup otherwise — see `resolve_kv_cache_types` in
/// `gglib-runtime` for the escape hatches).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KvCacheType {
    F32,
    F16,
    Bf16,
    Q8_0,
    Q5_1,
    Q5_0,
    Q4_1,
    Q4_0,
}

impl KvCacheType {
    /// The value passed on the command line (matches llama.cpp's own
    /// `ggml_type_name`).
    #[must_use]
    pub const fn as_llama_arg(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F16 => "f16",
            Self::Bf16 => "bf16",
            Self::Q8_0 => "q8_0",
            Self::Q5_1 => "q5_1",
            Self::Q5_0 => "q5_0",
            Self::Q4_1 => "q4_1",
            Self::Q4_0 => "q4_0",
        }
    }

    /// `(block_bytes, block_elems)` — the on-disk/in-memory layout ggml uses
    /// for this type. Unquantized types are a trivial one-element block;
    /// quantized types pack `block_elems` values into `block_bytes` bytes
    /// (a shared scale/min plus packed sub-byte values).
    #[must_use]
    pub const fn block_layout(self) -> (u64, u64) {
        match self {
            Self::F32 => (4, 1),
            Self::F16 | Self::Bf16 => (2, 1),
            Self::Q8_0 => (34, 32),
            Self::Q5_1 => (24, 32),
            Self::Q5_0 => (22, 32),
            Self::Q4_1 => (20, 32),
            Self::Q4_0 => (18, 32),
        }
    }

    /// Estimated bytes to store `elems` elements at this type, rounding up
    /// to whole blocks (a partial trailing block still costs a full block).
    #[must_use]
    pub const fn bytes_for_elems(self, elems: u64) -> u64 {
        let (block_bytes, block_elems) = self.block_layout();
        elems.div_ceil(block_elems) * block_bytes
    }
}

impl FromStr for KvCacheType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "f32" => Ok(Self::F32),
            "f16" => Ok(Self::F16),
            "bf16" => Ok(Self::Bf16),
            "q8_0" => Ok(Self::Q8_0),
            "q5_1" => Ok(Self::Q5_1),
            "q5_0" => Ok(Self::Q5_0),
            "q4_1" => Ok(Self::Q4_1),
            "q4_0" => Ok(Self::Q4_0),
            other => Err(format!(
                "unknown KV cache type {other:?} (expected one of: f32, f16, bf16, q8_0, q5_1, q5_0, q4_1, q4_0)"
            )),
        }
    }
}

/// Default K cache type: quantized to roughly halve the KV footprint versus
/// `f16`, doubling how much conversation history the RAM/disk prompt caches
/// can hold.
pub const DEFAULT_CACHE_TYPE_K: KvCacheType = KvCacheType::Q8_0;

/// Default V cache type. Same rationale as [`DEFAULT_CACHE_TYPE_K`]; see the
/// module docs on the Flash Attention requirement for quantized V.
pub const DEFAULT_CACHE_TYPE_V: KvCacheType = KvCacheType::Q8_0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_llama_arg_matches_ggml_type_names() {
        assert_eq!(KvCacheType::F16.as_llama_arg(), "f16");
        assert_eq!(KvCacheType::Q8_0.as_llama_arg(), "q8_0");
    }

    #[test]
    fn from_str_round_trips_through_as_llama_arg() {
        for t in [
            KvCacheType::F32,
            KvCacheType::F16,
            KvCacheType::Bf16,
            KvCacheType::Q8_0,
            KvCacheType::Q5_1,
            KvCacheType::Q5_0,
            KvCacheType::Q4_1,
            KvCacheType::Q4_0,
        ] {
            assert_eq!(KvCacheType::from_str(t.as_llama_arg()), Ok(t));
        }
    }

    #[test]
    fn from_str_is_case_insensitive_and_trims() {
        assert_eq!(KvCacheType::from_str(" Q8_0 "), Ok(KvCacheType::Q8_0));
        assert_eq!(KvCacheType::from_str("F16"), Ok(KvCacheType::F16));
    }

    #[test]
    fn from_str_rejects_unknown_type() {
        assert!(KvCacheType::from_str("q2_k").is_err());
    }

    #[test]
    fn f16_bytes_for_elems_is_two_bytes_per_element() {
        assert_eq!(KvCacheType::F16.bytes_for_elems(1000), 2000);
    }

    #[test]
    fn q8_0_bytes_for_elems_matches_ggml_block_layout() {
        // Exactly one block: 32 elements -> 34 bytes.
        assert_eq!(KvCacheType::Q8_0.bytes_for_elems(32), 34);
        // Two full blocks: 64 elements -> 68 bytes.
        assert_eq!(KvCacheType::Q8_0.bytes_for_elems(64), 68);
    }

    #[test]
    fn bytes_for_elems_rounds_up_a_partial_trailing_block() {
        // 33 elements needs 2 blocks of 32 at q8_0 -> 68 bytes, not 34+.
        assert_eq!(KvCacheType::Q8_0.bytes_for_elems(33), 68);
    }

    #[test]
    fn q8_0_is_smaller_than_f16_for_the_same_element_count() {
        let elems = 65_536;
        assert!(KvCacheType::Q8_0.bytes_for_elems(elems) < KvCacheType::F16.bytes_for_elems(elems));
    }

    #[test]
    fn cache_ram_setting_default_is_auto() {
        assert_eq!(CacheRamSetting::default(), CacheRamSetting::Auto);
    }
}
