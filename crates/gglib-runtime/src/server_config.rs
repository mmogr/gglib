//! Canonical [`ServerConfig`] builder for all llama-server launch surfaces.
//!
//! ## Why this module exists
//!
//! Multiple surfaces in gglib can trigger a llama-server launch:
//! - The **GUI/HTTP** start-server endpoint (`gglib-app-services`)
//! - The **CLI** agent-chat / question commands (`gglib-cli`)
//! - The **proxy** auto-start path (`gglib-runtime` `ProcessManager`)
//!
//! Without a shared builder, each surface independently assembled a
//! [`ServerConfig`], leading to capability drift — features such as MTP
//! speculative decoding, reasoning-format detection, and Jinja templates
//! were applied inconsistently depending on which surface triggered the
//! start.
//!
//! [`build_server_config`] is the **single source of truth** for translating
//! a model's tags and optional caller overrides into a fully-resolved
//! [`ServerConfig`].  All surfaces must go through this function; adding a
//! new capability resolver here automatically propagates parity to every
//! launch path.
//!
//! ## Capability detection precedence
//!
//! | Feature | Explicit override wins over… | Tag-based default |
//! |---------|------------------------------|-------------------|
//! | Jinja templates | `opts.jinja = Some(…)` | `"agent"` tag → enabled |
//! | Reasoning format | `opts.reasoning_format = Some(…)` | model tags |
//! | MTP speculative decoding | `opts.mtp_draft_n_max = Some(0)` (off) or `Some(n)` (on) | `"mtp"` tag → enabled |

use std::path::PathBuf;

use gglib_core::domain::InferenceConfig;
use gglib_core::ports::ServerConfig;
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;
use tracing::debug;

use crate::llama::args::{resolve_jinja_flag, resolve_mtp_args, resolve_reasoning_format};

// =============================================================================
// Options
// =============================================================================

/// Caller-supplied overrides for [`build_server_config`].
///
/// All fields default to `None`, which means "auto-detect from model tags".
/// Explicit values always take precedence over tag detection.
#[derive(Debug, Clone, Default)]
pub struct ServerConfigOptions {
    /// Override the context window size forwarded to llama-server.
    /// `None` lets llama-server use its built-in default.
    pub context_size: Option<u64>,

    /// Per-model server defaults context length (from Model.server_defaults.context_length).
    /// Second tier in fallback chain.
    pub model_server_ctx: Option<usize>,

    /// Global app setting for default context size (from Settings.default_context_size).
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
// Builder
// =============================================================================

/// Resolve context size using the 4-level fallback chain.
/// 1. Runtime request / CLI flag (opts.context_size) — highest priority
/// 2. Per-model server defaults (opts.model_server_ctx) — from DB
/// 3. Global app setting (opts.global_default_ctx)
/// 4. Hardcoded default (DEFAULT_CONTEXT_SIZE = 4096) — lowest priority
pub fn resolve_context_size(opts: &ServerConfigOptions) -> u64 {
    opts.context_size
        .or_else(|| opts.model_server_ctx.map(|v| v as u64))
        .or(opts.global_default_ctx)
        .unwrap_or(DEFAULT_CONTEXT_SIZE)
}

/// Build a [`ServerConfig`] from model identity, model tags, and caller options.
///
/// This is the **canonical entry point** for constructing a [`ServerConfig`] and
/// **must** be used by all launch surfaces to guarantee that the same model
/// always receives the same llama-server arguments regardless of which surface
/// triggered the start.
///
/// # Arguments
///
/// * `model_id` — Unique numeric model identifier (database row id).
/// * `model_name` — Human-readable model name forwarded to the process manager.
/// * `model_path` — Absolute path to the GGUF model file.
/// * `base_port` — Base port for llama-server port allocation.  Pass `0` when
///   the underlying [`GuiProcessCore`] allocates the port itself.
/// * `tags` — Model capability tags (e.g. `["mtp", "agent", "reasoning"]`).
///   Used for all tag-based auto-detection when the corresponding option field
///   is `None`.
/// * `opts` — Caller-supplied overrides.  Use
///   `ServerConfigOptions::default()` for fully automatic tag-based
///   configuration.
pub fn build_server_config(
    model_id: i64,
    model_name: String,
    model_path: PathBuf,
    base_port: u16,
    tags: &[String],
    opts: ServerConfigOptions,
) -> ServerConfig {
    let mut config = ServerConfig::new(model_id, model_name, model_path, base_port);

    // --- Context size (4-level fallback chain) --------------------------------
    let ctx = resolve_context_size(&opts);
    config = config.with_context_size(ctx);

    if let Some(port) = opts.port {
        config = config.with_port(port);
    }

    // --- Jinja templates -------------------------------------------------------
    let jinja = resolve_jinja_flag(opts.jinja, tags);
    if jinja.enabled {
        debug!(source = ?jinja.source, "enabling --jinja for model");
        config = config.with_jinja();
    }

    // --- Reasoning format ------------------------------------------------------
    match opts.reasoning_format.as_deref() {
        Some("none") => {
            // Caller explicitly suppressed reasoning — don't set the flag.
            debug!("reasoning format explicitly suppressed by caller");
        }
        Some(format) => {
            // Caller provided an explicit format string — use it directly.
            debug!(format, "using explicit reasoning format");
            config = config.with_reasoning_format(format.to_owned());
        }
        None => {
            // Auto-detect from model tags.
            let reasoning = resolve_reasoning_format(None, tags);
            if let Some(format) = reasoning.format {
                debug!(
                    format = %format,
                    source = ?reasoning.source,
                    "auto-detected reasoning format from model tags"
                );
                config = config.with_reasoning_format(format);
            }
        }
    }

    // --- Inference parameters --------------------------------------------------
    if let Some(params) = opts.inference_params {
        config = config.with_inference_config(params);
    }

    // --- MTP speculative decoding ----------------------------------------------
    let mtp = resolve_mtp_args(opts.mtp_draft_n_max, opts.mtp_draft_p_min, tags);
    if mtp.enabled {
        debug!(
            n_max = mtp.draft_n_max,
            p_min = mtp.draft_p_min,
            source = ?mtp.source,
            "enabling MTP speculative decoding"
        );
        config = config
            .with_spec_draft_n_max(mtp.draft_n_max)
            .with_spec_draft_p_min(mtp.draft_p_min);
    }

    config
}
