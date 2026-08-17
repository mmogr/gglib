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
//! | Jinja templates | `opts.jinja = Some(true)` → `--jinja`, `Some(false)` → `--no-jinja` | `"agent"` tag → `--jinja`; otherwise **no flag**, and llama-server's own default (jinja on) stands |
//! | Reasoning format | `opts.reasoning_format = Some(…)` | model tags |
//! | MTP speculative decoding | `opts.mtp_draft_n_max = Some(0)` (off) or `Some(n)` (on) | `"mtp"` tag → enabled |
//! | Embedding mode | *(no override — see below)* | `"embedding"` tag → enabled |
//!
//! The jinja row is the one where gglib's silence is not a "no". llama-server
//! starts with `use_jinja = true` and the server example never clears it, so an
//! untagged model launches *with* jinja unless something outside gglib says
//! otherwise. That is why the flag is emitted in both directions: `--jinja`
//! states a decision gglib already agreed with, and `--no-jinja` is the only
//! thing gglib can send that takes jinja away. Nothing tag-derived emits
//! `--no-jinja` — removing tool-call templating and template kwargs from every
//! non-agent model is not a default anyone asked for, so only an explicit
//! override reaches it.
//!
//! The "outside gglib" caveat is real: llama.cpp registers `LLAMA_ARG_JINJA`
//! as an env alias for the same option, and gglib does not sanitise the
//! spawned process's environment. An exported `LLAMA_ARG_JINJA=0` therefore
//! turns jinja off under the deferred arm. It cannot override the explicit
//! arm — command-line arguments are applied after the environment — so
//! `--no-jinja` still means what it says.
//!
//! Embedding mode is the one row with no override column. `--embeddings`
//! restricts llama-server to serving embeddings, so the flag is a statement
//! about what the model is rather than a preference: forcing it on for a chat
//! model yields a server that refuses chat completions, and forcing it off for
//! an embedding model yields one that 501s on `/v1/embeddings`.
//!
//! ## One translator
//!
//! [`build_server_config`] is the sole translator from options to
//! llama-server arguments. Callers that carry a
//! [`UnifiedServerConfig`](crate::unified_server_config::UnifiedServerConfig)
//! flatten its tiers with `resolved_options()` first; the process manager
//! then calls this function once, at spawn (see
//! [`ResidentSet`](crate::process::ResidentSet)).
//!
//! Jinja, reasoning format, MTP and KV cache types are resolved here and
//! nowhere else — duplicating those resolvers is precisely the drift this
//! module exists to prevent.

use std::path::PathBuf;

use gglib_core::ports::ServerConfig;
// `ServerConfigOptions` stays `pub` because `lib.rs` re-exports it — a re-export
// chain must be public the whole way, and demoting this link is E0365. The lint
// tracks the chain correctly; it only fires if the root re-export is missing.
//
// Split from `resolve_context_size` for that reason. The two shared one
// statement, nothing re-exports or names the second through this crate, and the
// `pub` the lint offers to demote covers the whole line — so the two names had
// to part company before either could be right.
pub use gglib_core::server_config::ServerConfigOptions;
pub(crate) use gglib_core::server_config::resolve_context_size;
use tracing::debug;

use crate::llama::args::{
    JinjaResolution, MtpResolution, ReasoningFormatResolution, ReasoningFormatSource,
    resolve_embeddings_flag, resolve_jinja_flag, resolve_kv_cache_types, resolve_mtp_args,
    resolve_reasoning_format,
};

/// The capability resolutions [`build_server_config`] performed, handed back
/// so a caller can explain the launch it just configured.
///
/// These decisions are taken here and nowhere else (see the module docs), so
/// this is the only place their `*Source` provenance exists. Returning it
/// rather than re-resolving at the call site is deliberate: a second
/// resolution that drifted would narrate a launch that never happened.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedCapabilities {
    /// Which jinja flag was emitted — `--jinja`, `--no-jinja`, or neither —
    /// and why.
    pub jinja: JinjaResolution,
    /// The `--reasoning-format` decision, and why.
    pub reasoning: ReasoningFormatResolution,
    /// The MTP speculative-decoding decision, and why.
    pub mtp: MtpResolution,
    /// Whether `--embeddings` was emitted. Tag-derived, so it carries no
    /// `*Source` of its own — there is only one way for it to be true.
    pub embeddings: bool,
}

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
///   the underlying [`crate::process::GuiProcessCore`] allocates the port itself.
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
    build_server_config_narrated(model_id, model_name, model_path, base_port, tags, opts).0
}

/// [`build_server_config`], additionally returning the capability
/// resolutions it performed so the caller can narrate them.
///
/// Same translation, same single source of truth — `build_server_config` is a
/// projection of this function that drops the explanation.
pub(crate) fn build_server_config_narrated(
    model_id: i64,
    model_name: String,
    model_path: PathBuf,
    base_port: u16,
    tags: &[String],
    opts: ServerConfigOptions,
) -> (ServerConfig, ResolvedCapabilities) {
    let mut config = ServerConfig::new(model_id, model_name, model_path, base_port);

    // --- Context size (4-level fallback chain) --------------------------------
    let ctx = resolve_context_size(&opts);
    config = config.with_context_size(ctx);

    if let Some(port) = opts.port {
        config = config.with_port(port);
    }

    // --- Jinja templates -------------------------------------------------------
    // Carried through as a mode, unconditionally: the "no flag" outcome is a
    // decision in its own right here (see the module docs), so there is nothing
    // to branch on.
    let jinja = resolve_jinja_flag(opts.jinja, tags);
    debug!(mode = ?jinja.mode, source = ?jinja.source, "resolved jinja mode for model");
    config = config.with_jinja_mode(jinja.mode);

    // --- Reasoning format ------------------------------------------------------
    let reasoning = match opts.reasoning_format.as_deref() {
        Some("none") => {
            // Caller explicitly suppressed reasoning — don't set the flag.
            debug!("reasoning format explicitly suppressed by caller");
            ReasoningFormatResolution {
                format: None,
                source: ReasoningFormatSource::Explicit,
            }
        }
        Some(format) => {
            // Caller provided an explicit format string — use it directly.
            debug!(format, "using explicit reasoning format");
            config = config.with_reasoning_format(format.to_owned());
            ReasoningFormatResolution {
                format: Some(format.to_owned()),
                source: ReasoningFormatSource::Explicit,
            }
        }
        None => {
            // Auto-detect from model tags.
            let reasoning = resolve_reasoning_format(None, tags);
            if let Some(format) = reasoning.format.clone() {
                debug!(
                    format = %format,
                    source = ?reasoning.source,
                    "auto-detected reasoning format from model tags"
                );
                config = config.with_reasoning_format(format);
            }
            reasoning
        }
    };

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

    // --- Embedding mode (--embeddings) -----------------------------------------
    // Tag-derived only, with no `opts` field to override it: the flag restricts
    // llama-server to embeddings, so letting a caller force it either way would
    // only ever produce a server that refuses the requests it is about to get.
    let embeddings = resolve_embeddings_flag(tags);
    if embeddings {
        debug!("enabling --embeddings for model tagged embedding");
        config = config.with_embeddings();
    }

    (
        config,
        ResolvedCapabilities {
            jinja,
            reasoning,
            mtp,
            embeddings,
        },
    )
}

#[cfg(test)]
#[path = "server_config_tests.rs"]
mod server_config_tests;
