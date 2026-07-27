//! Inputs to [`start_proxy_standalone`](super::start_proxy_standalone).
//!
//! Split out of `proxy/mod.rs` both to keep that module to its startup
//! sequence and because these are the types the CLI builds directly — `gglib
//! proxy` and `gglib serve` each assemble one of these and hand it over.
//!
//! ## One entry point, two modes
//!
//! [`StandaloneProxyParams::pinned`] is the only thing separating the two
//! commands:
//!
//! | Field | `gglib proxy` | `gglib serve <model>` |
//! |-------|---------------|------------------------|
//! | `pinned` | `None` — auto-swap on request | `Some(PinnedModel)` — refuse others |
//!
//! Everything else — the Axum layer, cache lifecycle, dashboard, SSE, MCP
//! gateway, council wiring, shutdown — is shared verbatim. That is the point
//! of epic #630: `serve` is a *mode* of the proxy, not a second stack.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::cache_config::KvCacheType;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::{ModelRepository, SettingsRepository};
use gglib_core::server_config::ServerConfigOptions;
use gglib_mcp::McpService;

use crate::unified_server_config::default_slot_dir;

/// The model a pinned server is locked to.
///
/// Its presence turns the standalone proxy into `gglib serve`: the process
/// manager refuses every other model rather than swapping to it (see
/// [`ProcessManager::new_pinned`](crate::process::ProcessManager::new_pinned)).
#[derive(Debug, Clone)]
pub struct PinnedModel {
    /// Database row id of the pinned model.
    pub id: i64,
    /// Name clients must address the model by. Matched exactly.
    pub name: String,
    /// Standing launch options for this model, already resolved through the
    /// 3-tier cascade — normally
    /// [`UnifiedServerConfig::resolved_options`](crate::unified_server_config::UnifiedServerConfig::resolved_options).
    pub launch_overrides: ServerConfigOptions,
}

/// KV cache configuration for a standalone proxy run.
#[derive(Debug, Clone, Default)]
pub struct ProxyCacheOptions {
    /// Master switch for disk slot persistence. `false` means no
    /// `--slot-save-path` is ever emitted, whatever [`Self::slot_dir`] holds.
    pub enabled: bool,
    /// Directory for KV cache slot files. Only consulted when
    /// [`Self::enabled`]; `None` falls back to
    /// [`default_slot_dir`](crate::unified_server_config::default_slot_dir).
    pub slot_dir: Option<PathBuf>,
    /// RAM budget in MiB for llama-server's host-RAM prompt cache
    /// (`--cache-ram`). Independent of [`Self::enabled`]. `None` auto-sizes
    /// from system RAM, model weights and KV footprint.
    pub ram_mb: Option<u64>,
    /// Minimum chunk size in tokens for KV-shift cache reuse
    /// (`--cache-reuse`). `None` leaves it off.
    pub reuse: Option<u32>,
    /// Byte budget in GiB for the on-disk slot eviction sweep. `None`
    /// auto-sizes from free disk space.
    pub disk_gb: Option<u64>,
    /// Explicit K cache element type. `None` resolves to the `q8_0` default.
    pub type_k: Option<KvCacheType>,
    /// Explicit V cache element type. Same resolution as [`Self::type_k`].
    pub type_v: Option<KvCacheType>,
}

impl ProxyCacheOptions {
    /// The concrete slot directory, or `None` when disk persistence is off.
    ///
    /// [`Self::enabled`] wins over [`Self::slot_dir`] unconditionally, so
    /// "cache off" means zero cache flags downstream even when a directory was
    /// supplied.
    #[must_use]
    pub fn resolved_slot_dir(&self) -> Option<PathBuf> {
        self.enabled
            .then(|| self.slot_dir.clone().unwrap_or_else(default_slot_dir))
    }
}

/// Compose the process manager's standing launch options.
///
/// The cache settings are model-independent and always apply. A pinned run
/// additionally contributes its model's fully-cascaded options, which is how
/// `gglib serve --mlock` and friends reach llama-server at all; an unpinned
/// proxy has no model in scope at startup, so those are resolved per request
/// when a model is first named.
pub(super) fn compose_launch_overrides(
    cache: &ProxyCacheOptions,
    pinned: Option<&PinnedModel>,
    slot_save_path: Option<PathBuf>,
) -> ServerConfigOptions {
    let cache_opts = ServerConfigOptions {
        slot_save_path,
        cache_reuse: cache.reuse,
        cache_type_k: cache.type_k,
        cache_type_v: cache.type_v,
        ..Default::default()
    };

    match pinned {
        Some(model) => cache_opts.overlay(&model.launch_overrides),
        None => cache_opts,
    }
}

/// Everything [`start_proxy_standalone`](super::start_proxy_standalone) needs.
///
/// Replaces what was a 16-argument function, which had grown past the point
/// where positional calls were readable and carried a
/// `clippy::too_many_arguments` allow to prove it.
pub struct StandaloneProxyParams {
    /// Host the proxy binds to.
    pub host: String,
    /// Port the proxy binds to.
    pub port: u16,
    /// Base port for llama-server instances.
    pub llama_base_port: u16,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Model repository backing the catalog.
    pub model_repo: Arc<dyn ModelRepository>,
    /// MCP service for the tool gateway.
    pub mcp: Arc<McpService>,
    /// Settings repository for global inference defaults.
    pub settings_repo: Arc<dyn SettingsRepository>,
    /// Context size used when a client does not specify one.
    pub default_context: u64,
    /// Once-off sampling overrides applied above the persisted defaults.
    pub inference_override: Option<InferenceConfig>,
    /// KV cache configuration.
    pub cache: ProxyCacheOptions,
    /// `Some` runs the proxy pinned to one model (`gglib serve`); `None` runs
    /// the ordinary auto-swapping proxy (`gglib proxy`).
    pub pinned: Option<PinnedModel>,
}

impl StandaloneProxyParams {
    /// Name of the pinned model, if this run is pinned.
    ///
    /// Used for the startup banner and to decide which `ProcessManager`
    /// constructor to reach for.
    #[must_use]
    pub fn pinned_name(&self) -> Option<&str> {
        self.pinned.as_ref().map(|p| p.name.as_str())
    }

    /// Standing launch options for the process manager.
    ///
    /// See [`compose_launch_overrides`] for what each mode contributes.
    #[must_use]
    pub fn launch_overrides(&self, slot_save_path: Option<PathBuf>) -> ServerConfigOptions {
        compose_launch_overrides(&self.cache, self.pinned.as_ref(), slot_save_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pinned_model() -> PinnedModel {
        PinnedModel {
            id: 7,
            name: "qwen2.5".to_string(),
            launch_overrides: ServerConfigOptions {
                mlock: Some(true),
                jinja: Some(true),
                ..Default::default()
            },
        }
    }

    // ---------------------------------------------------------------
    // Launch-options composition
    // ---------------------------------------------------------------

    /// The pinned model's cascaded options must survive into the manager's
    /// template — this is the path by which `gglib serve --mlock` reaches
    /// llama-server at all.
    #[test]
    fn pinned_overrides_carry_the_models_options() {
        let opts =
            compose_launch_overrides(&ProxyCacheOptions::default(), Some(&pinned_model()), None);

        assert_eq!(opts.mlock, Some(true));
        assert_eq!(opts.jinja, Some(true));
    }

    /// An unpinned proxy has no model at startup, so it contributes no
    /// model-specific options — those are resolved per request instead.
    #[test]
    fn unpinned_overrides_carry_no_model_options() {
        let opts = compose_launch_overrides(&ProxyCacheOptions::default(), None, None);

        assert_eq!(opts.mlock, None);
        assert_eq!(opts.jinja, None);
    }

    /// Cache settings are model-independent, so they reach the template
    /// identically in both modes.
    #[test]
    fn cache_settings_reach_the_template_in_both_modes() {
        let cache = ProxyCacheOptions {
            reuse: Some(256),
            type_k: Some(KvCacheType::F16),
            ..Default::default()
        };
        let slot = PathBuf::from("/slots");
        let model = pinned_model();

        for pinned in [Some(&model), None] {
            let opts = compose_launch_overrides(&cache, pinned, Some(slot.clone()));
            assert_eq!(opts.slot_save_path, Some(slot.clone()));
            assert_eq!(opts.cache_reuse, Some(256));
            assert_eq!(opts.cache_type_k, Some(KvCacheType::F16));
        }
    }

    /// A pinned model's own cache opinion outranks the run-wide default,
    /// matching the tier order the cascade already establishes.
    #[test]
    fn pinned_model_options_outrank_run_wide_cache_settings() {
        let cache = ProxyCacheOptions {
            reuse: Some(256),
            ..Default::default()
        };
        let model = PinnedModel {
            launch_overrides: ServerConfigOptions {
                cache_reuse: Some(512),
                ..Default::default()
            },
            ..pinned_model()
        };

        let opts = compose_launch_overrides(&cache, Some(&model), None);

        assert_eq!(opts.cache_reuse, Some(512));
    }

    // ---------------------------------------------------------------
    // Slot directory resolution
    // ---------------------------------------------------------------

    #[test]
    fn slot_dir_resolves_when_cache_enabled() {
        let cache = ProxyCacheOptions {
            enabled: true,
            slot_dir: Some(PathBuf::from("/custom/slots")),
            ..Default::default()
        };
        assert_eq!(
            cache.resolved_slot_dir(),
            Some(PathBuf::from("/custom/slots"))
        );
    }

    #[test]
    fn slot_dir_falls_back_to_the_default_directory() {
        let cache = ProxyCacheOptions {
            enabled: true,
            ..Default::default()
        };
        assert_eq!(cache.resolved_slot_dir(), Some(default_slot_dir()));
    }

    /// The master switch beats an explicitly supplied directory, so `--cache`
    /// off emits no cache flags at all.
    #[test]
    fn disabled_cache_resolves_to_no_slot_dir() {
        let cache = ProxyCacheOptions {
            enabled: false,
            slot_dir: Some(PathBuf::from("/custom/slots")),
            ..Default::default()
        };
        assert_eq!(cache.resolved_slot_dir(), None);
    }
}
