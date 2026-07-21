//! KV-cache quantization (`--cache-type-k` / `--cache-type-v`) argument
//! resolution.
//!
//! Defaults both K and V to `q8_0`, roughly halving KV cache bytes-per-token
//! versus llama.cpp's own `f16` default — doubling how much conversation
//! history the RAM/disk prompt caches can hold at the same memory budget.
//!
//! ## The Flash Attention constraint
//!
//! llama.cpp hard-errors at startup if V is quantized while Flash Attention
//! resolves off (gglib leaves `--flash-attn` at llama.cpp's own `auto`), and
//! also if a type's block size doesn't evenly divide the model's per-head
//! dimension. If a launch hits either case, override the failing axis
//! (`--cache-type-v f16` is the usual fix) or set `GGLIB_DISABLE_KV_QUANT=1`
//! to fall back to `f16`/`f16` entirely.

use crate::system::is_truthy_flag;
use gglib_core::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V, KvCacheType};

/// Indicates how the K/V cache types were resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvCacheTypeSource {
    /// User explicitly supplied `--cache-type-k`/`--cache-type-v` (per axis).
    Explicit,
    /// The `q8_0` default.
    Default,
    /// Quantization suppressed via `GGLIB_DISABLE_KV_QUANT` — `f16` applies
    /// to any axis not explicitly overridden.
    DisabledByEnv,
}

/// Resolved K/V cache types for a llama-server launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvCacheTypeResolution {
    pub k: KvCacheType,
    pub v: KvCacheType,
    /// Why `k`/`v` were chosen; per-axis explicit overrides collapse to
    /// [`KvCacheTypeSource::Explicit`] as a whole for simplicity, since the
    /// common case sets both axes together.
    pub source: KvCacheTypeSource,
}

impl KvCacheTypeResolution {
    /// A one-line, human-readable explanation, or `None` when both types are
    /// the plain default (nothing unusual to explain).
    #[must_use]
    pub fn explain(&self) -> Option<String> {
        match self.source {
            KvCacheTypeSource::Default => None,
            KvCacheTypeSource::DisabledByEnv => Some(format!(
                "KV cache quantization disabled via GGLIB_DISABLE_KV_QUANT: cache-type-k={}, cache-type-v={}",
                self.k.as_llama_arg(),
                self.v.as_llama_arg(),
            )),
            KvCacheTypeSource::Explicit => Some(format!(
                "KV cache types: cache-type-k={}, cache-type-v={} (explicit override)",
                self.k.as_llama_arg(),
                self.v.as_llama_arg(),
            )),
        }
    }
}

/// Whether `GGLIB_DISABLE_KV_QUANT` requests that quantization be suppressed,
/// falling back to `f16` for any non-explicit axis.
///
/// Same `GGLIB_DISABLE_<FEATURE>` convention as `GGLIB_DISABLE_MTP` and
/// `GGLIB_DISABLE_CACHE_AUTOSIZE`; the escape hatch for the Flash Attention
/// constraint documented in the module docs.
fn kv_quant_disabled_via_env() -> bool {
    std::env::var("GGLIB_DISABLE_KV_QUANT")
        .ok()
        .is_some_and(|v| is_truthy_flag(&v))
}

/// Resolve the `--cache-type-k`/`--cache-type-v` types for a llama-server launch.
///
/// Resolution order (highest priority first), applied independently per axis:
///
/// 1. **Explicit** — an explicit `explicit_k`/`explicit_v` value wins for
///    that axis unconditionally.
/// 2. **`GGLIB_DISABLE_KV_QUANT`** — any non-explicit axis falls back to `f16`.
/// 3. **Default** — `q8_0` for both axes.
#[must_use]
pub fn resolve_kv_cache_types(
    explicit_k: Option<KvCacheType>,
    explicit_v: Option<KvCacheType>,
) -> KvCacheTypeResolution {
    resolve_kv_cache_types_inner(explicit_k, explicit_v, kv_quant_disabled_via_env())
}

/// Pure core of [`resolve_kv_cache_types`], with the env lookup lifted into a
/// parameter so the kill-switch behaviour is testable without mutating
/// process-global environment state.
fn resolve_kv_cache_types_inner(
    explicit_k: Option<KvCacheType>,
    explicit_v: Option<KvCacheType>,
    quant_disabled: bool,
) -> KvCacheTypeResolution {
    if explicit_k.is_some() || explicit_v.is_some() {
        return KvCacheTypeResolution {
            k: explicit_k.unwrap_or(if quant_disabled {
                KvCacheType::F16
            } else {
                DEFAULT_CACHE_TYPE_K
            }),
            v: explicit_v.unwrap_or(if quant_disabled {
                KvCacheType::F16
            } else {
                DEFAULT_CACHE_TYPE_V
            }),
            source: KvCacheTypeSource::Explicit,
        };
    }

    if quant_disabled {
        return KvCacheTypeResolution {
            k: KvCacheType::F16,
            v: KvCacheType::F16,
            source: KvCacheTypeSource::DisabledByEnv,
        };
    }

    KvCacheTypeResolution {
        k: DEFAULT_CACHE_TYPE_K,
        v: DEFAULT_CACHE_TYPE_V,
        source: KvCacheTypeSource::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_q8_0_on_both_axes() {
        let got = resolve_kv_cache_types_inner(None, None, false);
        assert_eq!(got.k, KvCacheType::Q8_0);
        assert_eq!(got.v, KvCacheType::Q8_0);
        assert_eq!(got.source, KvCacheTypeSource::Default);
    }

    #[test]
    fn explicit_k_overrides_only_k() {
        let got = resolve_kv_cache_types_inner(Some(KvCacheType::F32), None, false);
        assert_eq!(got.k, KvCacheType::F32);
        assert_eq!(got.v, KvCacheType::Q8_0);
        assert_eq!(got.source, KvCacheTypeSource::Explicit);
    }

    #[test]
    fn explicit_v_overrides_only_v() {
        let got = resolve_kv_cache_types_inner(None, Some(KvCacheType::F16), false);
        assert_eq!(got.k, KvCacheType::Q8_0);
        assert_eq!(got.v, KvCacheType::F16);
        assert_eq!(got.source, KvCacheTypeSource::Explicit);
    }

    #[test]
    fn explicit_both_axes_honored() {
        let got = resolve_kv_cache_types_inner(Some(KvCacheType::Q4_0), Some(KvCacheType::F16), false);
        assert_eq!(got.k, KvCacheType::Q4_0);
        assert_eq!(got.v, KvCacheType::F16);
    }

    #[test]
    fn kill_switch_forces_f16_on_non_explicit_axes() {
        let got = resolve_kv_cache_types_inner(None, None, true);
        assert_eq!(got.k, KvCacheType::F16);
        assert_eq!(got.v, KvCacheType::F16);
        assert_eq!(got.source, KvCacheTypeSource::DisabledByEnv);
    }

    /// An explicit value outranks the kill switch on that axis — the user
    /// asked for a specific type, and suppressing the default shouldn't
    /// discard it.
    #[test]
    fn kill_switch_does_not_override_an_explicit_axis() {
        let got = resolve_kv_cache_types_inner(Some(KvCacheType::Q8_0), None, true);
        assert_eq!(got.k, KvCacheType::Q8_0);
        // v wasn't explicit, so the kill switch still forces f16 for it.
        assert_eq!(got.v, KvCacheType::F16);
        assert_eq!(got.source, KvCacheTypeSource::Explicit);
    }

    #[test]
    fn explain_is_none_for_plain_default() {
        assert!(resolve_kv_cache_types_inner(None, None, false).explain().is_none());
    }

    #[test]
    fn explain_reports_the_kill_switch() {
        let msg = resolve_kv_cache_types_inner(None, None, true).explain().unwrap();
        assert!(msg.contains("GGLIB_DISABLE_KV_QUANT"), "{msg}");
        assert!(msg.contains("f16"), "{msg}");
    }

    #[test]
    fn explain_reports_an_explicit_override() {
        let msg = resolve_kv_cache_types_inner(Some(KvCacheType::F32), None, false)
            .explain()
            .unwrap();
        assert!(msg.contains("f32"), "{msg}");
    }
}
