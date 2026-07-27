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
//!
//! ## Two layers, one translator
//!
//! Callers reach this module through one of two entry points, and it matters
//! which:
//!
//! | Function | Answers | Use when |
//! |----------|---------|----------|
//! | [`resolve_unified_config`] | "which tier wins, *then* which flags?" | the caller holds a [`UnifiedServerConfig`] — CLI, GUI, proxy |
//! | [`build_server_config`] | "which flags?" | the caller has already flattened its options |
//!
//! [`resolve_unified_config`] is a *cascade* layer: it resolves tier
//! precedence and then hands the flattened result straight to
//! [`build_server_config`], which remains the sole translator from options to
//! llama-server arguments. It deliberately does not re-derive jinja, reasoning
//! format, MTP or KV cache types — duplicating those resolvers is precisely
//! the drift this module exists to prevent.

use std::path::PathBuf;

use gglib_core::ports::ServerConfig;
pub use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use tracing::debug;

use crate::llama::args::{
    resolve_jinja_flag, resolve_kv_cache_types, resolve_mtp_args, resolve_reasoning_format,
};
use crate::unified_server_config::UnifiedServerConfig;

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

    // --- Native RAM cache tuning (--cache-ram / --cache-reuse) ------------------
    // Direct pass-through, no tag-based auto-detection, and deliberately
    // independent of slot persistence above — see ServerConfig's field docs.
    if let Some(mb) = opts.cache_ram_mb {
        config = config.with_cache_ram_mb(mb);
    }
    if let Some(n) = opts.cache_reuse {
        config = config.with_cache_reuse(n);
    }

    // --- KV cache quantization (--cache-type-k / --cache-type-v) ---------------
    // Resolved here (not left as a raw pass-through like cache_ram_mb above) so
    // every launch surface gets the same q8_0 default without each caller
    // re-implementing the resolution — see `resolve_kv_cache_types`.
    let kv_types = resolve_kv_cache_types(opts.cache_type_k, opts.cache_type_v);
    if let Some(explanation) = kv_types.explain() {
        debug!("{explanation}");
    }
    config = config
        .with_cache_type_k(kv_types.k)
        .with_cache_type_v(kv_types.v);

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

    // --- Memory lock (--mlock) -------------------------------------------------
    if opts.mlock.unwrap_or(false) {
        config = config.with_mlock();
    }

    config
}

// =============================================================================
// Cascade entry point
// =============================================================================

/// Resolve a [`UnifiedServerConfig`] into a fully-resolved [`ServerConfig`].
///
/// This is the entry point every launch surface should reach for. It applies
/// the strict 3-tier cascade (explicit overrides > model defaults > global
/// defaults) and then delegates to [`build_server_config`] for the actual
/// translation into llama-server arguments.
///
/// The delegation is the point: capability detection stays defined exactly
/// once, so a resolver added to [`build_server_config`] reaches this path
/// automatically. See the [module docs](self) for how the two layers divide.
#[must_use]
pub fn resolve_unified_config(input: &UnifiedServerConfig) -> ServerConfig {
    build_server_config(
        input.model_id,
        input.model_name.clone(),
        input.model_path.clone(),
        input.globals.llama_base_port,
        &input.model_tags,
        input.resolved_options(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unified_server_config::GlobalDefaults;
    use gglib_core::domain::InferenceConfig;

    const BASE_PORT: u16 = 9000;

    fn model_path() -> PathBuf {
        PathBuf::from("/models/test.gguf")
    }

    /// Build both ways from the same inputs and compare.
    ///
    /// `ServerConfig` has no `PartialEq`, so the comparison goes through
    /// `Debug` — every field is represented there, so a dropped or altered
    /// field still shows up.
    fn assert_parity(tags: &[String], opts: ServerConfigOptions, globals: GlobalDefaults) {
        let direct = build_server_config(
            7,
            "parity-model".to_string(),
            model_path(),
            BASE_PORT,
            tags,
            opts.clone(),
        );

        let unified = resolve_unified_config(&UnifiedServerConfig {
            model_id: 7,
            model_name: "parity-model".to_string(),
            model_path: model_path(),
            model_tags: tags.to_vec(),
            explicit: opts,
            globals,
            pinned: false,
        });

        assert_eq!(format!("{direct:?}"), format!("{unified:?}"));
    }

    /// Tier 3 contributes nothing, so the cascade must be a pass-through and
    /// both entry points must agree exactly.
    #[test]
    fn parity_with_default_options() {
        assert_parity(
            &[],
            ServerConfigOptions::default(),
            GlobalDefaults {
                llama_base_port: BASE_PORT,
                ..Default::default()
            },
        );
    }

    #[test]
    fn parity_with_fully_specified_options() {
        let opts = ServerConfigOptions {
            context_size: Some(32_768),
            model_server_ctx: Some(16_384),
            global_default_ctx: Some(8192),
            port: Some(5501),
            jinja: Some(true),
            reasoning_format: Some("deepseek".to_string()),
            mtp_draft_n_max: Some(4),
            mtp_draft_p_min: Some(0.8),
            cache_ram_mb: Some(4096),
            cache_reuse: Some(256),
            inference_params: Some(InferenceConfig {
                temperature: Some(0.7),
                ..Default::default()
            }),
            mlock: Some(true),
            ..Default::default()
        };

        assert_parity(
            &["mtp".to_string(), "agent".to_string()],
            opts,
            GlobalDefaults {
                llama_base_port: BASE_PORT,
                ..Default::default()
            },
        );
    }

    /// Tag-driven auto-detection has to survive the extra layer — this is the
    /// capability drift the epic exists to prevent.
    #[test]
    fn parity_with_tag_driven_detection() {
        assert_parity(
            &[
                "mtp".to_string(),
                "agent".to_string(),
                "reasoning".to_string(),
            ],
            ServerConfigOptions::default(),
            GlobalDefaults {
                llama_base_port: BASE_PORT,
                ..Default::default()
            },
        );
    }

    /// With caching on, the slot directory reaches the built config the same
    /// way whether it arrived as an explicit option or a global default.
    #[test]
    fn parity_with_cache_enabled() {
        let slot_dir = PathBuf::from("/slots/parity");

        assert_parity(
            &[],
            ServerConfigOptions {
                slot_save_path: Some(slot_dir.clone()),
                ..Default::default()
            },
            GlobalDefaults {
                llama_base_port: BASE_PORT,
                cache_enabled: true,
                slot_dir: Some(slot_dir),
                ..Default::default()
            },
        );
    }

    // ---------------------------------------------------------------
    // The cascade is actually applied (not just passed through)
    // ---------------------------------------------------------------

    fn unified(explicit: ServerConfigOptions, globals: GlobalDefaults) -> ServerConfig {
        resolve_unified_config(&UnifiedServerConfig {
            model_id: 1,
            model_name: "m".to_string(),
            model_path: model_path(),
            model_tags: Vec::new(),
            explicit,
            globals,
            pinned: false,
        })
    }

    #[test]
    fn cascade_applies_global_context_when_nothing_explicit() {
        let config = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                default_ctx: Some(8192),
                ..Default::default()
            },
        );
        assert_eq!(config.context_size, Some(8192));
    }

    #[test]
    fn cascade_lets_explicit_context_beat_global() {
        let config = unified(
            ServerConfigOptions {
                context_size: Some(32_768),
                ..Default::default()
            },
            GlobalDefaults {
                default_ctx: Some(8192),
                ..Default::default()
            },
        );
        assert_eq!(config.context_size, Some(32_768));
    }

    #[test]
    fn cascade_carries_the_llama_base_port_from_globals() {
        let config = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                llama_base_port: 5500,
                ..Default::default()
            },
        );
        assert_eq!(config.base_port, 5500);
    }

    /// mlock reaching the built config is what #631 plumbed; this asserts the
    /// new cascade layer did not sever it.
    #[test]
    fn cascade_carries_mlock_through_to_the_built_config() {
        let config = unified(
            ServerConfigOptions {
                mlock: Some(true),
                ..Default::default()
            },
            GlobalDefaults::default(),
        );
        assert!(config.mlock);
    }

    #[test]
    fn cascade_suppresses_slot_path_when_cache_disabled() {
        let config = unified(
            ServerConfigOptions {
                slot_save_path: Some(PathBuf::from("/slots/ignored")),
                ..Default::default()
            },
            GlobalDefaults {
                cache_enabled: false,
                ..Default::default()
            },
        );
        assert_eq!(config.slot_save_path, None);
    }
}
