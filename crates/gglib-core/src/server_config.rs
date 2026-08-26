//! Canonical context-size resolver (5-level fallback chain).
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
///
/// Serialized as part of the daemon's HTTP contract: a pinned proxy start
/// (`POST /api/proxy/start`) carries the model's fully-cascaded options in
/// the request body. `#[serde(default)]` keeps that contract stable when a
/// field is added — an older client's body simply resolves the new field to
/// `None`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ServerConfigOptions {
    /// Override the context window size forwarded to llama-server.
    /// `None` lets llama-server use its built-in default.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub context_size: Option<u64>,

    /// Per-model server defaults context length (from `Model.server_defaults.context_length`).
    /// Second tier in fallback chain.
    pub model_server_ctx: Option<usize>,

    /// Global app setting for default context size (from `Settings.default_context_size`).
    /// Third tier in fallback chain.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub global_default_ctx: Option<u64>,

    /// Context fitted to this model and this machine, from
    /// [`crate::domain::fit_context`]. Fourth tier in the fallback chain.
    ///
    /// `None` when it could not be computed — unknown KV shape, no memory
    /// reading — which is a refusal, not a zero: the chain falls through to the
    /// built-in default rather than launching against a guess.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub fitted_ctx: Option<u64>,

    /// Bind llama-server to a specific port instead of letting the allocator
    /// choose.
    pub port: Option<u16>,

    /// Override Jinja template support.
    /// - `None` → auto-detect: `--jinja` when the model has the `"agent"` tag,
    ///   and otherwise **no flag at all**, which leaves llama-server's own
    ///   default (jinja on) in place rather than disabling it.
    /// - `Some(true)` → `--jinja` regardless of tags.
    /// - `Some(false)` → `--no-jinja` regardless of tags. The only route to
    ///   actually turning jinja off; see [`crate::ports::JinjaMode`].
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
    /// (`--cache-ram`). `None` leaves llama-server's built-in default. `Some(0)`
    /// disables the cache. Direct pass-through, no tag-based auto-detection.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub cache_ram_mb: Option<u64>,

    /// Minimum chunk size in tokens for KV-shift cache reuse past the first
    /// prefix divergence point (`--cache-reuse`). `None` leaves the feature
    /// off. Direct pass-through, no tag-based auto-detection.
    pub cache_reuse: Option<u32>,

    /// Explicit override for the K cache element type (`--cache-type-k`).
    /// `None` resolves to the `q8_0` default (see
    /// `gglib_runtime::llama::args::resolve_kv_cache_types`), unless
    /// `GGLIB_DISABLE_KV_QUANT=1` is set.
    pub cache_type_k: Option<crate::cache_config::KvCacheType>,

    /// Explicit override for the V cache element type (`--cache-type-v`).
    /// Same resolution as [`Self::cache_type_k`]. Quantizing V additionally
    /// requires Flash Attention to be active — see
    /// `gglib_runtime::llama::args::kv_cache_type` module docs.
    pub cache_type_v: Option<crate::cache_config::KvCacheType>,

    /// Inference parameter overrides (temperature, top-p, etc.) forwarded
    /// directly to llama-server.
    pub inference_params: Option<InferenceConfig>,

    /// Whether to memory-lock the model into RAM (`--mlock`).
    /// `None` defaults to `false` in `build_server_config()`.
    pub mlock: Option<bool>,
}

impl ServerConfigOptions {
    /// Field-wise merge: every `Some` in `over` wins, every `None` falls
    /// through to `self`.
    ///
    /// This is the single layering primitive behind both places where two sets
    /// of options meet:
    ///
    /// - the 3-tier cascade in `UnifiedServerConfig::resolved_options`, where
    ///   global defaults are the base and explicit CLI/GUI overrides are `over`;
    /// - per-call launch overrides layered on top of a `ProcessManager`'s
    ///   standing template.
    ///
    /// Note that this merges *options*, not resolved values — the tier chain
    /// baked into [`resolve_context_size`] (request → per-model → global →
    /// fitted → hardcoded) still runs afterwards on the merged result, so
    /// overlaying never collapses those tiers early.
    ///
    /// `over` is destructured exhaustively on purpose: adding a field to this
    /// struct then fails to compile until it is given merge semantics here,
    /// rather than being silently dropped.
    #[must_use]
    pub fn overlay(&self, over: &Self) -> Self {
        let Self {
            context_size,
            model_server_ctx,
            global_default_ctx,
            fitted_ctx,
            port,
            jinja,
            reasoning_format,
            mtp_draft_n_max,
            mtp_draft_p_min,
            slot_save_path,
            cache_ram_mb,
            cache_reuse,
            cache_type_k,
            cache_type_v,
            inference_params,
            mlock,
        } = over;

        Self {
            context_size: context_size.or(self.context_size),
            model_server_ctx: model_server_ctx.or(self.model_server_ctx),
            global_default_ctx: global_default_ctx.or(self.global_default_ctx),
            fitted_ctx: fitted_ctx.or(self.fitted_ctx),
            port: port.or(self.port),
            jinja: jinja.or(self.jinja),
            reasoning_format: reasoning_format
                .clone()
                .or_else(|| self.reasoning_format.clone()),
            mtp_draft_n_max: mtp_draft_n_max.or(self.mtp_draft_n_max),
            mtp_draft_p_min: mtp_draft_p_min.or(self.mtp_draft_p_min),
            slot_save_path: slot_save_path
                .clone()
                .or_else(|| self.slot_save_path.clone()),
            cache_ram_mb: cache_ram_mb.or(self.cache_ram_mb),
            cache_reuse: cache_reuse.or(self.cache_reuse),
            cache_type_k: cache_type_k.or(self.cache_type_k),
            cache_type_v: cache_type_v.or(self.cache_type_v),
            inference_params: inference_params
                .clone()
                .or_else(|| self.inference_params.clone()),
            mlock: mlock.or(self.mlock),
        }
    }
}

// =============================================================================
// Resolver
// =============================================================================

/// Which rung of the context fallback chain supplied the resolved value.
///
/// Exists so a launch can state *why* it runs at a given context rather than
/// only what that context is — the number alone cannot distinguish a value
/// the user asked for from one inherited from the model's stored defaults or
/// from the 4096 floor. See [`crate::domain::LaunchNarration`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSizeSource {
    /// Runtime request or CLI flag (`opts.context_size`).
    Explicit,
    /// Per-model server defaults from the database (`opts.model_server_ctx`).
    ModelServerDefaults,
    /// Global app setting (`opts.global_default_ctx`).
    GlobalDefault,
    /// Computed from the model's trained context and this machine's memory.
    FittedToHardware,
    /// The hardcoded [`DEFAULT_CONTEXT_SIZE`] floor — nothing else was set.
    BuiltInDefault,
}

impl ContextSizeSource {
    /// Short label for display, e.g. `model server_defaults`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::ModelServerDefaults => "model server_defaults",
            Self::GlobalDefault => "global default",
            Self::FittedToHardware => "fitted to hardware",
            Self::BuiltInDefault => "built-in default",
        }
    }
}

/// Resolve context size using the 5-level fallback chain, reporting which
/// rung won.
///
/// 1. Runtime request / CLI flag (`opts.context_size`) — highest priority
/// 2. Per-model server defaults (`opts.model_server_ctx`) — from DB
/// 3. Global app setting (`opts.global_default_ctx`) — only when the user set one
/// 4. Fitted to this machine (`opts.fitted_ctx`)
/// 5. Hardcoded default (`DEFAULT_CONTEXT_SIZE` = 4096) — lowest priority
///
/// [`resolve_context_size`] delegates here and discards the source, so the
/// chain exists in exactly one place: a second copy that drifted would make
/// the banner explain a decision the launch did not actually take.
pub const fn resolve_context_size_with_source(
    opts: &ServerConfigOptions,
) -> (u64, ContextSizeSource) {
    if let Some(ctx) = opts.context_size {
        return (ctx, ContextSizeSource::Explicit);
    }
    if let Some(ctx) = opts.model_server_ctx {
        return (ctx as u64, ContextSizeSource::ModelServerDefaults);
    }
    if let Some(ctx) = opts.global_default_ctx {
        return (ctx, ContextSizeSource::GlobalDefault);
    }
    if let Some(ctx) = opts.fitted_ctx {
        return (ctx, ContextSizeSource::FittedToHardware);
    }
    (DEFAULT_CONTEXT_SIZE, ContextSizeSource::BuiltInDefault)
}

/// Resolve context size using the 5-level fallback chain.
/// 1. Runtime request / CLI flag (`opts.context_size`) — highest priority
/// 2. Per-model server defaults (`opts.model_server_ctx`) — from DB
/// 3. Global app setting (`opts.global_default_ctx`) — only when the user set one
/// 4. Fitted to this machine (`opts.fitted_ctx`)
/// 5. Hardcoded default (`DEFAULT_CONTEXT_SIZE` = 4096) — lowest priority
///
/// The fitted value sits *below* the global default deliberately: a number the
/// user typed outranks one gglib computed. It sits above the built-in floor for
/// the same reason — 4096 is what you serve when you know nothing, and by this
/// rung something is known.
pub const fn resolve_context_size(opts: &ServerConfigOptions) -> u64 {
    resolve_context_size_with_source(opts).0
}

// =============================================================================
// Host-RAM prompt cache budget (`--cache-ram`)
// =============================================================================

// `CacheRamSetting` now lives in `crate::cache_config`, alongside
// `KvCacheType` — cache-related config resolution has one home. Re-exported
// here so existing `gglib_core::server_config::CacheRamSetting` call sites
// keep working.
pub use crate::cache_config::CacheRamSetting;

// Cache-RAM budget constants and [`compute_auto_cache_ram_mb`] now live in
// `crate::domain::cache_budget` (re-exported from `crate::domain`), alongside
// the rest of the domain's pure calculations.
pub use crate::domain::cache_budget::{
    CACHE_RAM_FLOOR_BYTES, CACHE_RAM_HEADROOM_BYTES, CACHE_RAM_UNKNOWN_KV_ALLOWANCE_BYTES,
    compute_auto_cache_ram_mb,
};

#[cfg(test)]
mod tests {
    use crate::server_config::{ServerConfigOptions, resolve_context_size};
    use crate::settings::DEFAULT_CONTEXT_SIZE;

    #[test]
    fn test_resolve_context_size_default_when_all_none() {
        let opts = ServerConfigOptions::default();
        assert_eq!(resolve_context_size(&opts), DEFAULT_CONTEXT_SIZE);
    }

    use crate::server_config::{ContextSizeSource, resolve_context_size_with_source};

    /// Each rung wins in turn as the one above it is removed — this is the
    /// precedence the banner claims to be reporting.
    #[test]
    fn context_source_names_the_winning_rung_at_each_level() {
        let full = ServerConfigOptions {
            context_size: Some(32_768),
            model_server_ctx: Some(16_384),
            global_default_ctx: Some(8192),
            ..Default::default()
        };
        assert_eq!(
            resolve_context_size_with_source(&full),
            (32_768, ContextSizeSource::Explicit)
        );

        let no_explicit = ServerConfigOptions {
            context_size: None,
            ..full.clone()
        };
        assert_eq!(
            resolve_context_size_with_source(&no_explicit),
            (16_384, ContextSizeSource::ModelServerDefaults)
        );

        let global_only = ServerConfigOptions {
            context_size: None,
            model_server_ctx: None,
            ..full
        };
        assert_eq!(
            resolve_context_size_with_source(&global_only),
            (8192, ContextSizeSource::GlobalDefault)
        );

        assert_eq!(
            resolve_context_size_with_source(&ServerConfigOptions::default()),
            (DEFAULT_CONTEXT_SIZE, ContextSizeSource::BuiltInDefault)
        );
    }

    /// The bare resolver must stay a projection of the sourced one, or the
    /// banner would explain a decision the launch did not take.
    #[test]
    fn bare_resolver_agrees_with_the_sourced_one() {
        let opts = ServerConfigOptions {
            model_server_ctx: Some(16_384),
            global_default_ctx: Some(8192),
            ..Default::default()
        };
        assert_eq!(
            resolve_context_size(&opts),
            resolve_context_size_with_source(&opts).0
        );
    }

    // Cache-RAM budget math tests now live in
    // `crate::domain::cache_budget::tests`, alongside the function itself.
    use crate::server_config::CacheRamSetting;

    /// Every launch surface should auto-size unless it opts out, so `Auto`
    /// has to be the `Default` variant.
    #[test]
    fn cache_ram_setting_defaults_to_auto() {
        assert_eq!(CacheRamSetting::default(), CacheRamSetting::Auto);
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
    fn fitted_beats_the_built_in_default() {
        // The rung that makes the whole change worth anything: with nothing
        // configured, a machine-derived context is served instead of 4096.
        let opts = ServerConfigOptions {
            fitted_ctx: Some(32_768),
            ..Default::default()
        };
        let (ctx, source) = resolve_context_size_with_source(&opts);
        assert_eq!(ctx, 32_768);
        assert_eq!(source, ContextSizeSource::FittedToHardware);
    }

    #[test]
    fn a_user_set_global_default_beats_the_fitted_value() {
        // A number somebody typed outranks one gglib computed, even a worse
        // one — that is what "setting" means.
        let opts = ServerConfigOptions {
            global_default_ctx: Some(8192),
            fitted_ctx: Some(65_536),
            ..Default::default()
        };
        let (ctx, source) = resolve_context_size_with_source(&opts);
        assert_eq!(ctx, 8192);
        assert_eq!(source, ContextSizeSource::GlobalDefault);
    }

    #[test]
    fn per_model_server_defaults_beat_the_fitted_value() {
        let opts = ServerConfigOptions {
            model_server_ctx: Some(16_384),
            fitted_ctx: Some(65_536),
            ..Default::default()
        };
        assert_eq!(resolve_context_size(&opts), 16_384);
    }

    #[test]
    fn an_explicit_request_beats_the_fitted_value() {
        let opts = ServerConfigOptions {
            context_size: Some(4096),
            fitted_ctx: Some(65_536),
            ..Default::default()
        };
        assert_eq!(resolve_context_size(&opts), 4096);
    }

    #[test]
    fn the_built_in_default_survives_when_nothing_can_be_fitted() {
        // `fit_context` refuses rather than guessing, and a refusal must land
        // on the floor rather than on nothing.
        let opts = ServerConfigOptions {
            fitted_ctx: None,
            ..Default::default()
        };
        let (ctx, source) = resolve_context_size_with_source(&opts);
        assert_eq!(ctx, DEFAULT_CONTEXT_SIZE);
        assert_eq!(source, ContextSizeSource::BuiltInDefault);
    }

    #[test]
    fn overlay_carries_a_fitted_value_through() {
        let base = ServerConfigOptions {
            fitted_ctx: Some(32_768),
            ..Default::default()
        };
        assert_eq!(
            base.overlay(&ServerConfigOptions::default()).fitted_ctx,
            Some(32_768),
            "an empty per-call overlay must not erase the fitted value"
        );
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

    // -------------------------------------------------------------------
    // overlay
    // -------------------------------------------------------------------

    use crate::cache_config::KvCacheType;
    use crate::domain::InferenceConfig;
    use std::path::PathBuf;

    /// Every field set, so a merge that drops one is visible. `marker` is a
    /// `u8` purely so each field can widen losslessly via `From`.
    fn populated(marker: u8) -> ServerConfigOptions {
        ServerConfigOptions {
            context_size: Some(u64::from(marker)),
            model_server_ctx: Some(usize::from(marker)),
            global_default_ctx: Some(u64::from(marker)),
            fitted_ctx: Some(u64::from(marker)),
            port: Some(u16::from(marker)),
            jinja: Some(true),
            reasoning_format: Some(format!("fmt-{marker}")),
            mtp_draft_n_max: Some(u32::from(marker)),
            mtp_draft_p_min: Some(f32::from(marker)),
            slot_save_path: Some(PathBuf::from(format!("/slots/{marker}"))),
            cache_ram_mb: Some(u64::from(marker)),
            cache_reuse: Some(u32::from(marker)),
            cache_type_k: Some(KvCacheType::Q8_0),
            cache_type_v: Some(KvCacheType::F16),
            inference_params: Some(InferenceConfig {
                temperature: Some(f32::from(marker)),
                ..Default::default()
            }),
            mlock: Some(true),
        }
    }

    /// A fully-populated `over` must win on every single field. Compared
    /// field-by-field rather than wholesale so a failure names the culprit.
    #[test]
    fn overlay_over_wins_on_every_field() {
        let merged = populated(1).overlay(&populated(2));

        assert_eq!(merged.context_size, Some(2));
        assert_eq!(merged.model_server_ctx, Some(2));
        assert_eq!(merged.global_default_ctx, Some(2));
        assert_eq!(merged.port, Some(2));
        assert_eq!(merged.jinja, Some(true));
        assert_eq!(merged.reasoning_format.as_deref(), Some("fmt-2"));
        assert_eq!(merged.mtp_draft_n_max, Some(2));
        assert_eq!(merged.mtp_draft_p_min, Some(2.0));
        assert_eq!(merged.slot_save_path, Some(PathBuf::from("/slots/2")));
        assert_eq!(merged.cache_ram_mb, Some(2));
        assert_eq!(merged.cache_reuse, Some(2));
        assert_eq!(merged.cache_type_k, Some(KvCacheType::Q8_0));
        assert_eq!(merged.cache_type_v, Some(KvCacheType::F16));
        assert_eq!(
            merged.inference_params.and_then(|c| c.temperature),
            Some(2.0)
        );
        assert_eq!(merged.mlock, Some(true));
    }

    /// The direction that actually does the work — and the identity property
    /// the cascade leans on when a tier has no opinion: a base with values and
    /// an `over` that is silent must keep every base value.
    #[test]
    fn overlay_falls_through_to_base_on_every_field() {
        let merged = populated(1).overlay(&ServerConfigOptions::default());

        assert_eq!(merged.context_size, Some(1));
        assert_eq!(merged.model_server_ctx, Some(1));
        assert_eq!(merged.global_default_ctx, Some(1));
        assert_eq!(merged.port, Some(1));
        assert_eq!(merged.jinja, Some(true));
        assert_eq!(merged.reasoning_format.as_deref(), Some("fmt-1"));
        assert_eq!(merged.mtp_draft_n_max, Some(1));
        assert_eq!(merged.mtp_draft_p_min, Some(1.0));
        assert_eq!(merged.slot_save_path, Some(PathBuf::from("/slots/1")));
        assert_eq!(merged.cache_ram_mb, Some(1));
        assert_eq!(merged.cache_reuse, Some(1));
        assert_eq!(merged.cache_type_k, Some(KvCacheType::Q8_0));
        assert_eq!(merged.cache_type_v, Some(KvCacheType::F16));
        assert_eq!(
            merged.inference_params.and_then(|c| c.temperature),
            Some(1.0)
        );
        assert_eq!(merged.mlock, Some(true));
    }

    /// Per-field interleaving: neither side wholesale-replaces the other.
    #[test]
    fn overlay_merges_per_field_not_wholesale() {
        let base = ServerConfigOptions {
            context_size: Some(8192),
            mlock: Some(true),
            ..Default::default()
        };
        let over = ServerConfigOptions {
            port: Some(5500),
            mlock: Some(false),
            ..Default::default()
        };

        let merged = base.overlay(&over);

        assert_eq!(merged.context_size, Some(8192), "base-only field survives");
        assert_eq!(merged.port, Some(5500), "over-only field lands");
        assert_eq!(merged.mlock, Some(false), "contested field goes to over");
    }

    /// `Some(false)` is an explicit opinion, not an absence — it has to beat a
    /// `Some(true)` underneath it. This is what lets `--mtp-draft-n-max 0` and
    /// an explicit jinja-off override a tag-derived default.
    #[test]
    fn overlay_treats_some_false_as_an_override() {
        let base = ServerConfigOptions {
            jinja: Some(true),
            ..Default::default()
        };
        let over = ServerConfigOptions {
            jinja: Some(false),
            ..Default::default()
        };

        assert_eq!(base.overlay(&over).jinja, Some(false));
    }
}
