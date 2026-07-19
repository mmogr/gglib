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

use gglib_core::ports::ServerConfig;
pub use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use tracing::debug;

use crate::llama::args::{resolve_jinja_flag, resolve_mtp_args, resolve_reasoning_format};

// =============================================================================
// Builder
// =============================================================================

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

    // --- KV cache slot persistence ----------------------------------------------
    // Direct pass-through, no tag-based auto-detection: `None` here means the
    // feature is disabled and `build_and_spawn` emits zero cache-related flags,
    // leaving every existing model launch byte-for-byte unchanged.
    config = config.with_slot_save_path(opts.slot_save_path);

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
