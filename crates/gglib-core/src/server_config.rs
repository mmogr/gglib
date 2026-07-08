//! Canonical context-size resolver (4-level fallback chain).
//!
//! Extracted to `gglib-core` so that crates which cannot depend on
//! `gglib-runtime` (e.g. `gglib-proxy`) can still use the same resolution
//! logic for idle-model advertisements in `/v1/models`.

use crate::domain::InferenceConfig;
use crate::settings::DEFAULT_CONTEXT_SIZE;

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

#[cfg(test)]
mod tests {
    use crate::server_config::{ServerConfigOptions, resolve_context_size};
    use crate::settings::DEFAULT_CONTEXT_SIZE;

    #[test]
    fn test_resolve_context_size_default_when_all_none() {
        let opts = ServerConfigOptions::default();
        assert_eq!(resolve_context_size(&opts), DEFAULT_CONTEXT_SIZE);
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
}
