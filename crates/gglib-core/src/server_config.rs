//! Canonical context-size resolver (4-level fallback chain).
//!
//! Extracted to `gglib-core` so that crates which cannot depend on
//! `gglib-runtime` (e.g. `gglib-proxy`) can still use the same resolution
//! logic for idle-model advertisements in `/v1/models`.

use anyhow::{Result, anyhow};
use std::path::PathBuf;

use crate::domain::InferenceConfig;
use crate::settings::DEFAULT_CONTEXT_SIZE;

// =============================================================================
// CLI flag parsing (deferred resolution)
// =============================================================================

/// A parsed `--ctx-size` CLI flag, before it is resolved against model
/// metadata.
///
/// CLI argument parsing happens before the model is fetched from the
/// database, so the raw flag cannot be resolved to a concrete value at parse
/// time. [`CtxSizeArg::parse`] only validates the *shape* of the flag
/// (numeric or the literal `max`); callers must call [`CtxSizeArg::resolve`]
/// once the model (and its GGUF `context_length`) is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtxSizeArg {
    /// User passed `max` — resolve against the model's GGUF context length.
    Max,
    /// User passed an explicit numeric value.
    Value(u64),
}

impl CtxSizeArg {
    /// Parse a raw `--ctx-size` flag value.
    ///
    /// Accepts a positive integer or the case-insensitive literal `max`.
    /// Anything else is a hard error — invalid input must never be
    /// silently ignored.
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("max") {
            return Ok(Self::Max);
        }
        trimmed.parse::<u64>().map(Self::Value).map_err(|_| {
            anyhow!("Invalid context size '{trimmed}'. Use a positive number or 'max'")
        })
    }

    /// Resolve this flag into a concrete context size, now that the model's
    /// GGUF metadata is available.
    ///
    /// - `Max` resolves to `model_max_ctx` (`None` if the model has no
    ///   recorded context length — falls through to the next tier).
    /// - `Value(n)` always resolves to `Some(n)`.
    pub const fn resolve(self, model_max_ctx: Option<u64>) -> Option<u64> {
        match self {
            Self::Max => model_max_ctx,
            Self::Value(v) => Some(v),
        }
    }
}

/// Parse an optional raw `--ctx-size` flag into a [`CtxSizeArg`].
///
/// Convenience wrapper for CLI call sites: `None` (flag omitted) stays
/// `None`; `Some(raw)` is parsed and propagates a hard error on invalid
/// input via `?`.
pub fn parse_ctx_size_flag(raw: Option<&str>) -> Result<Option<CtxSizeArg>> {
    raw.map(CtxSizeArg::parse).transpose()
}

// =============================================================================
// Options
// =============================================================================

/// Caller-supplied overrides for [`resolve_context_size`].
///
/// All fields default to `None`, which means "fall through to next tier".
#[derive(Debug, Clone, Default)]
pub struct ServerConfigOptions {
    /// Override the context window size forwarded to llama-server.
    /// `None` lets llama-server use its built-in default.
    pub context_size: Option<u64>,

    /// Per-model server defaults context length (from `Model.server_defaults.context_length`).
    /// Second tier in fallback chain.
    pub model_server_ctx: Option<usize>,

    /// Global app setting for default context size (from `Settings.default_context_size`).
    /// Third tier in fallback chain.
    pub global_default_ctx: Option<u64>,

    /// Bind llama-server to a specific port instead of letting the allocator
    /// choose.
    pub port: Option<u16>,

    /// Override Jinja template support.
    /// - `None` → auto-detect: enabled when the model has the `"agent"` tag.
    /// - `Some(true)` → force enable regardless of tags.
    /// - `Some(false)` → force disable regardless of tags.
    pub jinja: Option<bool>,

    /// Override the reasoning format passed to llama-server.
    /// - `None` → auto-detect from model tags (e.g. `"reasoning"` tag).
    /// - `Some("none")` → explicitly suppress reasoning extraction even if the
    ///   model has a reasoning tag.
    /// - `Some("deepseek")` / `Some("deepseek-legacy")` → force a specific
    ///   format.
    pub reasoning_format: Option<String>,

    /// Override the MTP draft token count.
    /// - `None` → auto-detect: enabled with default `n=2` when the model has
    ///   the `"mtp"` tag.
    /// - `Some(0)` → explicitly disable MTP even if the model has the `"mtp"`
    ///   tag.
    /// - `Some(n)` → enable MTP with `n` draft tokens.
    pub mtp_draft_n_max: Option<u32>,

    /// Override the MTP acceptance probability threshold.
    /// Only meaningful when MTP is enabled. `None` uses the default (`0.75`).
    pub mtp_draft_p_min: Option<f32>,

    /// Directory for llama-server KV cache slot persistence (`--slot-save-path`).
    /// - `None` — disk slot persistence disabled, no `--slot-save-path` flag.
    /// - `Some(dir)` — enables slot save/restore.
    ///   Direct pass-through, no tag-based auto-detection (unlike jinja/MTP/reasoning).
    ///   Independent of `cache_ram_mb`/`cache_reuse` below.
    pub slot_save_path: Option<PathBuf>,

    /// RAM budget in MiB for llama-server's own host-RAM prompt cache
    /// (`--cache-ram`). `None` leaves llama-server's built-in default (or,
    /// for back-compat, `-1` when `slot_save_path` is set). Direct
    /// pass-through, no tag-based auto-detection.
    pub cache_ram_mb: Option<i64>,

    /// Minimum chunk size in tokens for KV-shift cache reuse past the first
    /// prefix divergence point (`--cache-reuse`). `None` leaves the feature
    /// off. Direct pass-through, no tag-based auto-detection.
    pub cache_reuse: Option<u32>,

    /// Inference parameter overrides (temperature, top-p, etc.) forwarded
    /// directly to llama-server.
    pub inference_params: Option<InferenceConfig>,
}

// =============================================================================
// Resolver
// =============================================================================

/// Resolve context size using the 4-level fallback chain.
/// 1. Runtime request / CLI flag (`opts.context_size`) — highest priority
/// 2. Per-model server defaults (`opts.model_server_ctx`) — from DB
/// 3. Global app setting (`opts.global_default_ctx`)
/// 4. Hardcoded default (`DEFAULT_CONTEXT_SIZE` = 4096) — lowest priority
pub fn resolve_context_size(opts: &ServerConfigOptions) -> u64 {
    opts.context_size
        .or_else(|| opts.model_server_ctx.map(|v| v as u64))
        .or(opts.global_default_ctx)
        .unwrap_or(DEFAULT_CONTEXT_SIZE)
}

// =============================================================================
// Host-RAM prompt cache budget (`--cache-ram`)
// =============================================================================

/// How to determine the host-RAM prompt cache budget (`--cache-ram`).
///
/// Deliberately a three-state enum rather than `Option<i64>`: "no explicit
/// value" is genuinely ambiguous between *auto-size me* (what the proxy wants)
/// and *emit nothing, leave llama-server's own default* (what benchmark
/// launches want — a large prompt cache would perturb throughput measurements
/// and RAM footprint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheRamSetting {
    /// Compute a budget from system RAM, model size, and the KV estimate.
    Auto,
    /// Use exactly this MiB value. llama-server's own sentinels pass through:
    /// `-1` unlimited, `0` disabled.
    Explicit(i64),
    /// Emit no `--cache-ram` flag at all; llama-server applies its built-in
    /// default (8192 MiB). The default variant, so existing callers that
    /// previously passed `None` keep byte-identical behaviour.
    #[default]
    LlamaDefault,
}

/// RAM reserved for the OS, other applications, and llama.cpp's own
/// compute/scratch buffers — never handed to the prompt cache.
pub const CACHE_RAM_HEADROOM_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Below this, a prompt cache holds too little to be worth the memory
/// pressure, so the budget collapses to `0` (explicitly disabled).
pub const CACHE_RAM_FLOOR_BYTES: u64 = 1024 * 1024 * 1024;

/// Caps the auto budget at `total_ram / CACHE_RAM_MAX_FRACTION_DIVISOR` (25%).
///
/// This cap, not the subtraction, is what binds on large machines, and it is
/// the primary safety margin against under-counted model weights (e.g. a
/// multi-part GGUF whose siblings weren't found).
pub const CACHE_RAM_MAX_FRACTION_DIVISOR: u64 = 4;

/// KV allowance assumed when the model's metadata doesn't permit an estimate.
/// Deliberately generous: over-reserving shrinks the cache (safe), whereas
/// under-reserving risks memory pressure.
pub const CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Compute the auto `--cache-ram` budget, in MiB.
///
/// ```text
/// usable  = total_ram − model_weights − kv_bytes − HEADROOM
/// budget  = min(usable, total_ram / 4)
/// result  = if budget < FLOOR { 0 } else { budget }
/// ```
///
/// Saturating throughout: a model larger than RAM yields `0` (cache disabled)
/// rather than wrapping into a huge budget.
///
/// # Arguments
///
/// * `total_ram_bytes` — total physical system RAM.
/// * `model_bytes` — on-disk size of the model weights (all shards).
/// * `kv_bytes` — estimated KV cache at the launch context size; pass
///   [`CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES`] when unknown.
#[must_use]
pub fn compute_auto_cache_ram_mb(total_ram_bytes: u64, model_bytes: u64, kv_bytes: u64) -> i64 {
    let reserved = model_bytes
        .saturating_add(kv_bytes)
        .saturating_add(CACHE_RAM_HEADROOM_BYTES);
    let usable = total_ram_bytes.saturating_sub(reserved);
    let cap = total_ram_bytes / CACHE_RAM_MAX_FRACTION_DIVISOR;

    let budget = usable.min(cap);
    if budget < CACHE_RAM_FLOOR_BYTES {
        return 0;
    }
    // Saturating rather than casting: `-1`/`0` are llama-server sentinels, so a
    // wrapped negative here would silently mean "unlimited".
    i64::try_from(budget / (1024 * 1024)).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use crate::server_config::{ServerConfigOptions, resolve_context_size};
    use crate::settings::DEFAULT_CONTEXT_SIZE;

    #[test]
    fn test_resolve_context_size_default_when_all_none() {
        let opts = ServerConfigOptions::default();
        assert_eq!(resolve_context_size(&opts), DEFAULT_CONTEXT_SIZE);
    }

    // ── Auto cache-ram budget ────────────────────────────────────────────

    use crate::server_config::{
        CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES, CacheRamSetting, compute_auto_cache_ram_mb,
    };

    const GIB: u64 = 1024 * 1024 * 1024;

    /// The reference case: 128 GiB machine, 27 GiB weights, ~9 GiB KV.
    /// Subtraction leaves ~76 GiB, so the 25% cap (32 GiB) binds.
    #[test]
    fn auto_budget_is_capped_at_a_quarter_of_ram() {
        let got = compute_auto_cache_ram_mb(128 * GIB, 27 * GIB, 9 * GIB);
        assert_eq!(got, 32 * 1024, "expected the 25% cap (32 GiB) to bind");
    }

    /// When free RAM is the binding constraint the subtraction wins, not the cap.
    #[test]
    fn auto_budget_uses_remaining_ram_when_below_the_cap() {
        // 64 - 30 - 4 - 16 = 14 GiB usable; cap would be 16 GiB.
        let got = compute_auto_cache_ram_mb(64 * GIB, 30 * GIB, 4 * GIB);
        assert_eq!(got, 14 * 1024);
    }

    /// A model that leaves under the 1 GiB floor disables the cache outright
    /// rather than letting llama-server apply its 8 GiB default.
    #[test]
    fn auto_budget_collapses_to_zero_under_the_floor() {
        // 36 - 20 - 4 - 16 = saturates to 0.
        assert_eq!(compute_auto_cache_ram_mb(36 * GIB, 20 * GIB, 4 * GIB), 0);
    }

    /// A model larger than total RAM must saturate to 0, never wrap around
    /// into an enormous budget.
    #[test]
    fn auto_budget_saturates_when_model_exceeds_ram() {
        assert_eq!(compute_auto_cache_ram_mb(16 * GIB, 64 * GIB, 8 * GIB), 0);
    }

    /// 8 GiB laptop: headroom (16 GiB) alone exceeds total RAM, so reserved
    /// saturates past the machine's capacity → budget collapses to 0.
    #[test]
    fn auto_budget_is_zero_on_small_ram_laptop() {
        // reserved = 3 + 0 + 16 = 19 > 8 → usable = 0
        assert_eq!(compute_auto_cache_ram_mb(8 * GIB, 3 * GIB, 0), 0);
    }

    /// 24 GiB machine: subtraction lands exactly on the 1 GiB floor.
    #[test]
    fn auto_budget_hits_floor_boundary_at_24_gib() {
        // reserved = 7 + 0 + 16 = 23; usable = 24 - 23 = 1 GiB
        // cap = 24/4 = 6 GiB; budget = min(1, 6) = 1 GiB → 1024 MiB
        assert_eq!(compute_auto_cache_ram_mb(24 * GIB, 7 * GIB, 0), 1024);
    }

    /// 32 GiB machine: subtraction binds (not the cap).
    #[test]
    fn auto_budget_subtraction_binds_on_32_gib_machine() {
        // reserved = 10 + 0 + 16 = 26; usable = 32 - 26 = 6 GiB
        // cap = 32/4 = 8 GiB; budget = min(6, 8) = 6 GiB → 6144 MiB
        assert_eq!(compute_auto_cache_ram_mb(32 * GIB, 10 * GIB, 0), 6144);
    }

    /// The unknown-KV allowance is generous enough to shrink, never inflate,
    /// the budget relative to a known small KV.
    #[test]
    fn unknown_kv_allowance_is_conservative() {
        let known_small = compute_auto_cache_ram_mb(64 * GIB, 10 * GIB, GIB);
        let unknown =
            compute_auto_cache_ram_mb(64 * GIB, 10 * GIB, CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES);
        assert!(
            unknown <= known_small,
            "unknown-KV budget {unknown} should not exceed known-KV {known_small}"
        );
    }

    /// Existing callers that previously passed `None` must keep emitting no
    /// flag at all, so `LlamaDefault` has to be the `Default` variant.
    #[test]
    fn cache_ram_setting_defaults_to_llama_default() {
        assert_eq!(CacheRamSetting::default(), CacheRamSetting::LlamaDefault);
    }

    #[test]
    fn test_resolve_context_size_global_beats_default() {
        let opts = ServerConfigOptions {
            global_default_ctx: Some(8192),
            ..Default::default()
        };
        assert_eq!(resolve_context_size(&opts), 8192);
    }

    #[test]
    fn test_resolve_context_size_model_beats_global() {
        let opts = ServerConfigOptions {
            model_server_ctx: Some(16_384),
            global_default_ctx: Some(8192),
            ..Default::default()
        };
        assert_eq!(resolve_context_size(&opts), 16_384);
    }

    #[test]
    fn test_resolve_context_size_runtime_beats_all() {
        let opts = ServerConfigOptions {
            context_size: Some(32_768),
            model_server_ctx: Some(16_384),
            global_default_ctx: Some(8192),
            ..Default::default()
        };
        assert_eq!(resolve_context_size(&opts), 32_768);
    }

    #[test]
    fn test_resolve_context_size_model_without_global() {
        let opts = ServerConfigOptions {
            model_server_ctx: Some(2048),
            ..Default::default()
        };
        assert_eq!(resolve_context_size(&opts), 2048);
    }

    #[test]
    fn test_resolve_context_size_zero_is_valid() {
        let opts = ServerConfigOptions {
            context_size: Some(0),
            ..Default::default()
        };
        assert_eq!(resolve_context_size(&opts), 0);
    }

    // -------------------------------------------------------------------
    // CtxSizeArg / parse_ctx_size_flag
    // -------------------------------------------------------------------

    use crate::server_config::{CtxSizeArg, parse_ctx_size_flag};

    #[test]
    fn ctx_size_arg_parses_explicit_numeric() {
        assert_eq!(CtxSizeArg::parse("8192").unwrap(), CtxSizeArg::Value(8192));
    }

    #[test]
    fn ctx_size_arg_parses_max_case_insensitive() {
        assert_eq!(CtxSizeArg::parse("max").unwrap(), CtxSizeArg::Max);
        assert_eq!(CtxSizeArg::parse("MAX").unwrap(), CtxSizeArg::Max);
        assert_eq!(CtxSizeArg::parse("  Max  ").unwrap(), CtxSizeArg::Max);
    }

    #[test]
    fn ctx_size_arg_invalid_string_is_hard_error() {
        assert!(CtxSizeArg::parse("banana").is_err());
    }

    #[test]
    fn ctx_size_arg_max_resolves_to_model_metadata() {
        assert_eq!(CtxSizeArg::Max.resolve(Some(131_072)), Some(131_072));
    }

    #[test]
    fn ctx_size_arg_max_without_model_metadata_resolves_to_none() {
        assert_eq!(CtxSizeArg::Max.resolve(None), None);
    }

    #[test]
    fn ctx_size_arg_value_ignores_model_metadata() {
        assert_eq!(CtxSizeArg::Value(4096).resolve(Some(131_072)), Some(4096));
    }

    #[test]
    fn parse_ctx_size_flag_none_when_flag_omitted() {
        assert_eq!(parse_ctx_size_flag(None).unwrap(), None);
    }

    #[test]
    fn parse_ctx_size_flag_propagates_parse_error() {
        assert!(parse_ctx_size_flag(Some("not-a-number")).is_err());
    }
}
