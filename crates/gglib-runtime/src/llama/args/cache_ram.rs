//! Host-RAM prompt cache (`--cache-ram`) argument resolution.
//!
//! Resolves the MiB budget for llama-server's own host-RAM prompt cache, which
//! is what makes switching between conversations fast: llama-server checkpoints
//! a slot's KV state into this cache and restores the best prefix match on the
//! next request, at memcpy speed.
//!
//! Left unset, llama-server applies an 8192 MiB default — often only two or
//! three large sessions' worth. [`CacheRamSetting::Auto`] instead derives a
//! budget from the machine's actual RAM, the model's weights, and its KV
//! footprint at the launch context size.

use crate::system::is_truthy_flag;
use gglib_core::server_config::{
    CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES, CacheRamSetting, compute_auto_cache_ram_mb,
};

/// Indicates how the `--cache-ram` budget was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheRamSource {
    /// User explicitly supplied a value (`--cache-ram-mb`); passed through
    /// verbatim, including llama-server's `-1`/`0` sentinels.
    Explicit,
    /// Computed from system RAM, model size, and the KV estimate.
    Auto,
    /// Auto-sizing was requested but suppressed via
    /// `GGLIB_DISABLE_CACHE_AUTOSIZE`; behaves as [`CacheRamSetting::LlamaDefault`].
    AutoSuppressedByEnv,
    /// No flag emitted — llama-server's built-in default applies.
    LlamaDefault,
}

/// Inputs and outcome of resolving the host-RAM prompt cache budget.
///
/// Carries the computed components (not just the answer) so the caller can log
/// a breakdown showing its work — an auto-chosen memory budget that appears
/// without explanation is impossible for a user to trust or debug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheRamResolution {
    /// Value for `--cache-ram`, or `None` to emit no flag.
    pub cache_ram_mb: Option<i64>,
    /// Why this value was chosen.
    pub source: CacheRamSource,
    /// Total system RAM in bytes (0 when not probed).
    pub total_ram_bytes: u64,
    /// Model weights in bytes, all shards (0 when unknown).
    pub model_bytes: u64,
    /// KV cache bytes assumed at the launch context size.
    pub kv_bytes: u64,
    /// Whether `kv_bytes` is a real estimate or the fallback allowance.
    pub kv_estimated: bool,
    /// Context size the KV figure was computed for.
    pub context_size: u64,
}

/// Whether `GGLIB_DISABLE_CACHE_AUTOSIZE` requests that auto-sizing be
/// suppressed, falling back to llama-server's own default.
///
/// Same `GGLIB_DISABLE_<FEATURE>` convention as `GGLIB_DISABLE_MTP` and
/// `GGLIB_DISABLE_CACHE_REUSE`; exists so a user can restore exactly the
/// pre-auto-sizing launch without editing config.
fn autosize_disabled_via_env() -> bool {
    std::env::var("GGLIB_DISABLE_CACHE_AUTOSIZE")
        .ok()
        .is_some_and(|v| is_truthy_flag(&v))
}

/// Resolve the `--cache-ram` budget for a llama-server launch.
///
/// Resolution order (highest priority first):
///
/// 1. **Explicit** — the user's value wins unconditionally.
/// 2. **`GGLIB_DISABLE_CACHE_AUTOSIZE`** — degrades `Auto` to no flag.
/// 3. **Auto** — computed budget (see
///    [`compute_auto_cache_ram_mb`]).
/// 4. **`LlamaDefault`** — no flag emitted.
///
/// # Arguments
///
/// * `setting` — what the caller asked for.
/// * `total_ram_bytes` — total physical RAM.
/// * `model_bytes` — model weights on disk, all shards.
/// * `kv_bytes_per_token` — per-token KV estimate; `None` substitutes a
///   conservative allowance.
/// * `context_size` — the context the server will actually launch with.
#[must_use]
pub fn resolve_cache_ram(
    setting: CacheRamSetting,
    total_ram_bytes: u64,
    model_bytes: u64,
    kv_bytes_per_token: Option<u64>,
    context_size: u64,
) -> CacheRamResolution {
    resolve_cache_ram_inner(
        setting,
        total_ram_bytes,
        model_bytes,
        kv_bytes_per_token,
        context_size,
        autosize_disabled_via_env(),
    )
}

/// Pure core of [`resolve_cache_ram`], with the env lookup lifted into a
/// parameter so the kill-switch behaviour is testable without mutating
/// process-global environment state.
fn resolve_cache_ram_inner(
    setting: CacheRamSetting,
    total_ram_bytes: u64,
    model_bytes: u64,
    kv_bytes_per_token: Option<u64>,
    context_size: u64,
    autosize_suppressed: bool,
) -> CacheRamResolution {
    let kv_estimated = kv_bytes_per_token.is_some();
    let kv_bytes = kv_bytes_per_token.map_or(CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES, |per_token| {
        gglib_core::domain::estimate_kv_bytes_for_context(per_token, context_size)
    });

    let base = CacheRamResolution {
        cache_ram_mb: None,
        source: CacheRamSource::LlamaDefault,
        total_ram_bytes,
        model_bytes,
        kv_bytes,
        kv_estimated,
        context_size,
    };

    match setting {
        // 1. Explicit always wins, sentinels included.
        CacheRamSetting::Explicit(mb) => CacheRamResolution {
            cache_ram_mb: Some(mb),
            source: CacheRamSource::Explicit,
            ..base
        },
        // 2. Kill switch: behave exactly as before auto-sizing existed.
        CacheRamSetting::Auto if autosize_suppressed => CacheRamResolution {
            source: CacheRamSource::AutoSuppressedByEnv,
            ..base
        },
        // 3. Compute a budget.
        CacheRamSetting::Auto => CacheRamResolution {
            cache_ram_mb: Some(compute_auto_cache_ram_mb(
                total_ram_bytes,
                model_bytes,
                kv_bytes,
            )),
            source: CacheRamSource::Auto,
            ..base
        },
        // 4. Emit nothing.
        CacheRamSetting::LlamaDefault => base,
    }
}

/// Format bytes as GiB with one decimal, for the human-facing breakdown.
fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

impl CacheRamResolution {
    /// A one-line, human-readable explanation of an auto-sized budget.
    ///
    /// Returns `None` for non-auto sources, which need no explanation.
    #[must_use]
    pub fn explain(&self) -> Option<String> {
        if self.source != CacheRamSource::Auto {
            return None;
        }
        let mb = self.cache_ram_mb?;
        let kv_label = if self.kv_estimated {
            "KV"
        } else {
            "KV (unknown, assumed)"
        };

        if mb == 0 {
            return Some(format!(
                "auto-sized llama-server RAM cache: disabled — model {} + {} {} at {} ctx leave no room in {}",
                gib(self.model_bytes),
                kv_label,
                gib(self.kv_bytes),
                self.context_size,
                gib(self.total_ram_bytes),
            ));
        }
        Some(format!(
            "auto-sized llama-server RAM cache: {mb} MiB (total {} − model {} − {} {} at {} ctx − headroom, capped at 25% of RAM) — override with --cache-ram-mb, disable with GGLIB_DISABLE_CACHE_AUTOSIZE=1",
            gib(self.total_ram_bytes),
            gib(self.model_bytes),
            kv_label,
            gib(self.kv_bytes),
            self.context_size,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    /// Reference machine: 128 GiB RAM, 27 GiB weights, 131k ctx.
    fn auto_on_reference_machine() -> CacheRamResolution {
        resolve_cache_ram(
            CacheRamSetting::Auto,
            128 * GIB,
            27 * GIB,
            Some(65_536), // 64 KiB/token → ~8.6 GiB at 131072 ctx
            131_072,
        )
    }

    #[test]
    fn explicit_value_wins_and_passes_through() {
        let got = resolve_cache_ram(
            CacheRamSetting::Explicit(4096),
            128 * GIB,
            27 * GIB,
            None,
            0,
        );
        assert_eq!(got.cache_ram_mb, Some(4096));
        assert_eq!(got.source, CacheRamSource::Explicit);
    }

    /// `-1` (unlimited) and `0` (disabled) are llama-server sentinels and must
    /// survive resolution untouched.
    #[test]
    fn explicit_sentinels_are_preserved() {
        for sentinel in [-1, 0] {
            let got = resolve_cache_ram(
                CacheRamSetting::Explicit(sentinel),
                128 * GIB,
                27 * GIB,
                None,
                4096,
            );
            assert_eq!(got.cache_ram_mb, Some(sentinel));
        }
    }

    #[test]
    fn llama_default_emits_no_flag() {
        let got = resolve_cache_ram(
            CacheRamSetting::LlamaDefault,
            128 * GIB,
            27 * GIB,
            None,
            4096,
        );
        assert_eq!(got.cache_ram_mb, None);
        assert_eq!(got.source, CacheRamSource::LlamaDefault);
    }

    #[test]
    fn auto_computes_a_capped_budget() {
        let got = auto_on_reference_machine();
        assert_eq!(got.source, CacheRamSource::Auto);
        // 25% of 128 GiB binds.
        assert_eq!(got.cache_ram_mb, Some(32 * 1024));
    }

    #[test]
    fn auto_uses_the_fallback_allowance_when_kv_is_unknown() {
        let got = resolve_cache_ram(CacheRamSetting::Auto, 128 * GIB, 27 * GIB, None, 131_072);
        assert!(!got.kv_estimated);
        assert_eq!(got.kv_bytes, CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES);
    }

    #[test]
    fn auto_multiplies_kv_by_context_size() {
        let got = resolve_cache_ram(CacheRamSetting::Auto, 64 * GIB, 10 * GIB, Some(1000), 8192);
        assert!(got.kv_estimated);
        assert_eq!(got.kv_bytes, 1000 * 8192);
    }

    /// Only auto-sized budgets get a user-facing explanation.
    #[test]
    fn explain_is_some_only_for_auto() {
        assert!(auto_on_reference_machine().explain().is_some());
        assert!(
            resolve_cache_ram(CacheRamSetting::Explicit(1), 0, 0, None, 0)
                .explain()
                .is_none()
        );
        assert!(
            resolve_cache_ram(CacheRamSetting::LlamaDefault, 0, 0, None, 0)
                .explain()
                .is_none()
        );
    }

    #[test]
    fn explain_shows_the_arithmetic() {
        let msg = auto_on_reference_machine().explain().unwrap();
        assert!(msg.contains("32768 MiB"), "{msg}");
        assert!(msg.contains("128.0 GiB"), "{msg}");
        assert!(msg.contains("27.0 GiB"), "{msg}");
        assert!(msg.contains("131072 ctx"), "{msg}");
        assert!(msg.contains("--cache-ram-mb"), "{msg}");
    }

    #[test]
    fn explain_flags_an_assumed_kv_figure() {
        let msg = resolve_cache_ram(CacheRamSetting::Auto, 128 * GIB, 27 * GIB, None, 131_072)
            .explain()
            .unwrap();
        assert!(msg.contains("unknown, assumed"), "{msg}");
    }

    #[test]
    fn explain_reports_a_disabled_budget() {
        let msg = resolve_cache_ram(CacheRamSetting::Auto, 36 * GIB, 30 * GIB, Some(0), 4096)
            .explain()
            .unwrap();
        assert!(msg.contains("disabled"), "{msg}");
    }

    /// The kill switch must produce byte-identical behaviour to
    /// `LlamaDefault` (no flag at all), not merely a smaller budget.
    #[test]
    fn kill_switch_suppresses_auto_sizing() {
        let got = resolve_cache_ram_inner(
            CacheRamSetting::Auto,
            128 * GIB,
            27 * GIB,
            Some(65_536),
            8192,
            true,
        );
        assert_eq!(got.cache_ram_mb, None);
        assert_eq!(got.source, CacheRamSource::AutoSuppressedByEnv);
    }

    /// An explicit value outranks the kill switch — the user asked for a
    /// specific budget, and suppressing autosizing shouldn't discard it.
    #[test]
    fn kill_switch_does_not_override_an_explicit_value() {
        let got = resolve_cache_ram_inner(
            CacheRamSetting::Explicit(2048),
            128 * GIB,
            27 * GIB,
            None,
            8192,
            true,
        );
        assert_eq!(got.cache_ram_mb, Some(2048));
        assert_eq!(got.source, CacheRamSource::Explicit);
    }

    #[test]
    fn truthy_flag_parsing_matches_the_other_kill_switches() {
        for v in ["1", "true", "TRUE", " yes ", "On"] {
            assert!(crate::system::is_truthy_flag(v), "{v:?} should be truthy");
        }
        for v in ["0", "false", "no", "off", "", "2"] {
            assert!(!crate::system::is_truthy_flag(v), "{v:?} should be falsy");
        }
    }
}
