//! Single input to the unified llama-server launch pipeline.
//!
//! ## Why this module exists
//!
//! Before this, proxy-level settings ([`ProxyConfig`]) and per-model launch
//! parameters ([`ServerConfigOptions`]) were two unrelated bags of options with
//! no enforced precedence between them. Fields such as `mlock`, `gpu_layers`
//! and `cache_ram_mb` were direct pass-throughs: whichever surface happened to
//! populate them won, and a surface that forgot silently got llama-server's
//! default instead of gglib's.
//!
//! [`UnifiedServerConfig`] carries the settings needed to launch a model —
//! whether the caller is `gglib serve`, `gglib proxy`, or the GUI — and
//! applies a strict 3-tier cascade to them.
//!
//! ## The cascade
//!
//! | Tier | Source | Where it lives |
//! |------|--------|----------------|
//! | 1 (wins) | Explicit overrides — CLI flags, GUI request fields | [`UnifiedServerConfig::explicit`] |
//! | 2 | Curated model defaults — GGUF metadata, model tags | `explicit.model_server_ctx`; tags, resolved by the caller |
//! | 3 (floor) | Global defaults — app settings, `127.0.0.1`, `--parallel 1` | [`UnifiedServerConfig::globals`] |
//!
//! Tier 2 is split across two places for a reason. The per-model context
//! length already has a home inside [`ServerConfigOptions`] (it is the second
//! rung of the context-resolution chain). The tag-driven defaults
//! for jinja, reasoning format and MTP are resolved from the model's own tags
//! by `build_server_config` — the caller already has the model in hand to get
//! `model_id`/`model_name`/`model_path` there in the first place, so this
//! struct does not duplicate them.
//!
//! ## Layering, not re-implementation
//!
//! This module resolves *tiers*; it does not translate options into
//! command-line flags. [`resolved_options`](UnifiedServerConfig::resolved_options)
//! flattens the three tiers into a single [`ServerConfigOptions`], which the
//! caller then hands to the canonical `build_server_config` alongside the
//! model's identity and tags. Adding a capability resolver stays a one-line
//! change in `build_server_config` and propagates here for free.

use std::path::PathBuf;

use gglib_core::domain::InferenceConfig;
use gglib_core::server_config::{
    ContextSizeSource, ServerConfigOptions, resolve_context_size_with_source,
};
use gglib_proxy::slot_eviction::DiskBudget;

use crate::proxy::ProxyConfig;

/// Tier 3 — the global floor every launch starts from.
///
/// These are the values that apply when neither the caller nor the model has
/// an opinion: the app's saved settings, plus gglib's own hardened defaults.
#[derive(Debug, Clone)]
pub struct GlobalDefaults {
    /// Host the HTTP surface binds to. Defaults to `127.0.0.1` — binding
    /// anywhere else is opt-in, never inherited.
    pub host: String,
    /// Port the proxy's own HTTP listener binds to.
    pub proxy_port: u16,
    /// Base port for llama-server allocation. Upstream instances are assigned
    /// sequentially from here.
    pub llama_base_port: u16,
    /// `Settings.default_context_size`. Third rung of the context chain.
    pub default_ctx: Option<u64>,
    /// Master switch for disk KV-slot persistence. When `false`, no
    /// `--slot-save-path` is emitted no matter what [`Self::slot_dir`] or the
    /// explicit tier says.
    pub cache_enabled: bool,
    /// Directory for KV cache slot files. Only consulted when
    /// [`Self::cache_enabled`]; `None` falls back to [`default_slot_dir`].
    pub slot_dir: Option<PathBuf>,
    /// Byte budget for the on-disk slot eviction sweep.
    pub disk_budget: DiskBudget,
    /// Operator-supplied sampling overrides for this process
    /// (`gglib proxy --temperature …`). Sits below the explicit tier.
    pub inference_override: Option<InferenceConfig>,
    /// Bearer token demanded of clients (`--api-key` / `GGLIB_API_KEY`).
    /// `None` defers to the stored setting, then to generating one for a
    /// non-loopback bind.
    pub api_key: Option<String>,
    /// Extra `Host` header values to accept (`--allowed-host`), beyond
    /// loopback and [`Self::host`].
    pub allowed_hosts: Vec<String>,
}

impl Default for GlobalDefaults {
    /// Derived from [`ProxyConfig::default`] rather than restating its values,
    /// so the hardened bind defaults have exactly one definition. Only the
    /// llama base port — which `ProxyConfig` has no concept of — is added.
    fn default() -> Self {
        let ProxyConfig {
            host,
            port,
            cache_enabled,
            slot_dir,
            disk_budget,
            inference_override,
            api_key,
            allowed_hosts,
            // Supplied by whoever starts the proxy, not a configured default:
            // only a daemon has a token to hand over.
            daemon_cancel: _,
            // Set per-caller on the start body, not inherited as a tier-3
            // default: `serve` takes it from the resolved profile selection
            // and the GUI from its own request.
            default_profile: _,
            // The proxy's fallback context is derived per-model by
            // `to_proxy_config`, so its default is not a tier-3 input.
            default_context: _,
        } = ProxyConfig::default();

        Self {
            host,
            proxy_port: port,
            llama_base_port: gglib_core::DEFAULT_LLAMA_BASE_PORT,
            default_ctx: None,
            cache_enabled,
            slot_dir,
            disk_budget,
            inference_override,
            api_key,
            allowed_hosts,
        }
    }
}

/// The KV-slot directory used when caching is on but no directory was chosen.
///
/// Falls back to a relative `slots` path only when the data root cannot be
/// resolved at all, matching what the standalone proxy has always done.
#[must_use]
pub fn default_slot_dir() -> PathBuf {
    gglib_core::paths::data_root()
        .map(|d| d.join("slots"))
        .unwrap_or_else(|_| PathBuf::from("slots"))
}

/// The launch settings that go through the 3-tier cascade, across every
/// surface.
///
/// Model identity (`model_id`/`model_name`/`model_path`/tags) and pinning are
/// deliberately not here: every real caller already has the `Model` domain
/// object and its own pinning decision in hand, and threading copies of both
/// through this struct as well would just be a second place for them to go
/// stale. See the [module docs](self) for the tier semantics.
#[derive(Debug, Clone)]
pub struct UnifiedServerConfig {
    /// Tier 1 — explicit overrides. Also carries the per-model context length
    /// (`model_server_ctx`), which is tier 2; see the module docs.
    pub explicit: ServerConfigOptions,

    /// Tier 3 — the global floor.
    pub globals: GlobalDefaults,
}

impl UnifiedServerConfig {
    /// Flatten all three tiers into one [`ServerConfigOptions`].
    ///
    /// Tier 3 is the base and tier 1 is overlaid on top, so an explicit `Some`
    /// always wins and an explicit `None` falls through to the global default.
    /// Tier 2's per-model context rides along inside the merged options
    /// (`model_server_ctx`); its tag-driven half is resolved by the caller
    /// passing the model's own tags to `build_server_config` alongside this.
    #[must_use]
    pub fn resolved_options(&self) -> ServerConfigOptions {
        let tier3 = ServerConfigOptions {
            global_default_ctx: self.globals.default_ctx,
            slot_save_path: self.resolved_slot_dir(),
            inference_params: self.globals.inference_override.clone(),
            ..Default::default()
        };

        let mut merged = tier3.overlay(&self.explicit);

        // `cache_enabled` is a master switch, not a tier: with caching off,
        // `--slot-save-path` must not be emitted even if the caller passed a
        // directory explicitly. Keeps "cache off" meaning byte-for-byte no
        // cache flags downstream.
        if !self.globals.cache_enabled {
            merged.slot_save_path = None;
        }

        merged
    }

    /// Derive the proxy-level configuration for this launch.
    ///
    /// `default_context` is the fully-resolved context size for *this* model
    /// rather than the bare global setting — in pinned mode the pinned model
    /// is the only thing the proxy will ever serve, so its resolved context is
    /// the only sensible fallback to advertise.
    #[must_use]
    pub fn to_proxy_config(&self) -> ProxyConfig {
        ProxyConfig {
            host: self.globals.host.clone(),
            port: self.globals.proxy_port,
            // Only a value somebody actually chose. Falling through to the
            // built-in floor here would hand the proxy `Some(4096)` and make
            // the fitted rung unreachable in pinned mode — the same laundering
            // this change removes from the ordinary path.
            default_context: match resolve_context_size_with_source(&self.resolved_options()) {
                (_, ContextSizeSource::BuiltInDefault) => None,
                (ctx, _) => Some(ctx),
            },
            cache_enabled: self.globals.cache_enabled,
            slot_dir: self.resolved_slot_dir(),
            disk_budget: self.globals.disk_budget,
            inference_override: self.globals.inference_override.clone(),
            default_profile: None,
            api_key: self.globals.api_key.clone(),
            allowed_hosts: self.globals.allowed_hosts.clone(),
            // Filled in by the daemon when it is the one starting this.
            daemon_cancel: None,
        }
    }

    /// The concrete slot directory, or `None` when disk persistence is off.
    fn resolved_slot_dir(&self) -> Option<PathBuf> {
        self.globals.cache_enabled.then(|| {
            self.globals
                .slot_dir
                .clone()
                .unwrap_or_else(default_slot_dir)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::cache_config::KvCacheType;
    use gglib_core::server_config::resolve_context_size;

    /// A config with nothing explicit set — every value comes from tier 3.
    fn bare(globals: GlobalDefaults) -> UnifiedServerConfig {
        UnifiedServerConfig {
            explicit: ServerConfigOptions::default(),
            globals,
        }
    }

    // ---------------------------------------------------------------
    // Context size — the one field with every rung of the chain in play
    // ---------------------------------------------------------------

    #[test]
    fn context_size_falls_to_global_default() {
        let cfg = bare(GlobalDefaults {
            default_ctx: Some(8192),
            ..Default::default()
        });
        assert_eq!(resolve_context_size(&cfg.resolved_options()), 8192);
    }

    #[test]
    fn context_size_model_default_beats_global() {
        let mut cfg = bare(GlobalDefaults {
            default_ctx: Some(8192),
            ..Default::default()
        });
        cfg.explicit.model_server_ctx = Some(16_384);
        assert_eq!(resolve_context_size(&cfg.resolved_options()), 16_384);
    }

    #[test]
    fn context_size_explicit_beats_model_and_global() {
        let mut cfg = bare(GlobalDefaults {
            default_ctx: Some(8192),
            ..Default::default()
        });
        cfg.explicit.model_server_ctx = Some(16_384);
        cfg.explicit.context_size = Some(32_768);
        assert_eq!(resolve_context_size(&cfg.resolved_options()), 32_768);
    }

    // ---------------------------------------------------------------
    // Tier 1 over tier 3, per field
    // ---------------------------------------------------------------

    #[test]
    fn explicit_inference_params_beat_global_override() {
        let global = InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        };
        let explicit = InferenceConfig {
            temperature: Some(0.9),
            ..Default::default()
        };

        let mut cfg = bare(GlobalDefaults {
            inference_override: Some(global),
            ..Default::default()
        });
        cfg.explicit.inference_params = Some(explicit);

        assert_eq!(
            cfg.resolved_options()
                .inference_params
                .and_then(|c| c.temperature),
            Some(0.9)
        );
    }

    #[test]
    fn global_inference_override_applies_when_no_explicit() {
        let cfg = bare(GlobalDefaults {
            inference_override: Some(InferenceConfig {
                temperature: Some(0.2),
                ..Default::default()
            }),
            ..Default::default()
        });

        assert_eq!(
            cfg.resolved_options()
                .inference_params
                .and_then(|c| c.temperature),
            Some(0.2)
        );
    }

    /// jinja, reasoning format and MTP have no tier-3 source — they are tier 1
    /// over tag-driven tier 2, and the tag half is resolved downstream. All
    /// this layer must do is carry the explicit value through untouched.
    #[test]
    fn tag_driven_fields_pass_through_untouched() {
        let mut cfg = bare(GlobalDefaults::default());
        cfg.explicit.jinja = Some(false);
        cfg.explicit.reasoning_format = Some("none".to_string());
        cfg.explicit.mtp_draft_n_max = Some(0);
        cfg.explicit.mlock = Some(true);

        let opts = cfg.resolved_options();

        assert_eq!(opts.jinja, Some(false));
        assert_eq!(opts.reasoning_format.as_deref(), Some("none"));
        assert_eq!(opts.mtp_draft_n_max, Some(0));
        assert_eq!(opts.mlock, Some(true));
    }

    #[test]
    fn mlock_absent_when_not_requested() {
        assert_eq!(
            bare(GlobalDefaults::default()).resolved_options().mlock,
            None
        );
    }

    #[test]
    fn explicit_cache_tuning_survives_the_cascade() {
        let mut cfg = bare(GlobalDefaults {
            cache_enabled: true,
            ..Default::default()
        });
        cfg.explicit.cache_ram_mb = Some(4096);
        cfg.explicit.cache_reuse = Some(256);
        cfg.explicit.cache_type_k = Some(KvCacheType::F16);
        cfg.explicit.cache_type_v = Some(KvCacheType::F16);

        let opts = cfg.resolved_options();

        assert_eq!(opts.cache_ram_mb, Some(4096));
        assert_eq!(opts.cache_reuse, Some(256));
        assert_eq!(opts.cache_type_k, Some(KvCacheType::F16));
        assert_eq!(opts.cache_type_v, Some(KvCacheType::F16));
    }

    // ---------------------------------------------------------------
    // cache_enabled master switch
    // ---------------------------------------------------------------

    #[test]
    fn slot_dir_resolves_from_globals_when_cache_enabled() {
        let cfg = bare(GlobalDefaults {
            cache_enabled: true,
            slot_dir: Some(PathBuf::from("/custom/slots")),
            ..Default::default()
        });
        assert_eq!(
            cfg.resolved_options().slot_save_path,
            Some(PathBuf::from("/custom/slots"))
        );
    }

    #[test]
    fn slot_dir_defaults_when_cache_enabled_without_a_directory() {
        let cfg = bare(GlobalDefaults {
            cache_enabled: true,
            ..Default::default()
        });
        assert_eq!(
            cfg.resolved_options().slot_save_path,
            Some(default_slot_dir())
        );
    }

    #[test]
    fn cache_disabled_emits_no_slot_path() {
        let cfg = bare(GlobalDefaults {
            cache_enabled: false,
            slot_dir: Some(PathBuf::from("/custom/slots")),
            ..Default::default()
        });
        assert_eq!(cfg.resolved_options().slot_save_path, None);
    }

    /// The master switch has to beat tier 1 too, or `--cache` off would leak
    /// a `--slot-save-path` whenever a caller pre-populated one.
    #[test]
    fn cache_disabled_overrides_an_explicit_slot_path() {
        let mut cfg = bare(GlobalDefaults {
            cache_enabled: false,
            ..Default::default()
        });
        cfg.explicit.slot_save_path = Some(PathBuf::from("/explicit/slots"));

        assert_eq!(cfg.resolved_options().slot_save_path, None);
    }

    #[test]
    fn explicit_slot_path_beats_global_when_cache_enabled() {
        let mut cfg = bare(GlobalDefaults {
            cache_enabled: true,
            slot_dir: Some(PathBuf::from("/global/slots")),
            ..Default::default()
        });
        cfg.explicit.slot_save_path = Some(PathBuf::from("/explicit/slots"));

        assert_eq!(
            cfg.resolved_options().slot_save_path,
            Some(PathBuf::from("/explicit/slots"))
        );
    }

    // ---------------------------------------------------------------
    // to_proxy_config
    // ---------------------------------------------------------------

    #[test]
    fn proxy_config_carries_the_global_bind_settings() {
        let cfg = bare(GlobalDefaults {
            host: "0.0.0.0".to_string(),
            proxy_port: 9999,
            ..Default::default()
        });
        let proxy = cfg.to_proxy_config();

        assert_eq!(proxy.host, "0.0.0.0");
        assert_eq!(proxy.port, 9999);
    }

    #[test]
    fn proxy_config_defaults_to_localhost() {
        assert_eq!(
            bare(GlobalDefaults::default()).to_proxy_config().host,
            "127.0.0.1"
        );
    }

    /// The proxy advertises the model's own resolved context, not the bare
    /// global setting — in pinned mode that is the only model it will serve.
    #[test]
    fn proxy_config_default_context_is_the_resolved_context() {
        let mut cfg = bare(GlobalDefaults {
            default_ctx: Some(4096),
            ..Default::default()
        });
        cfg.explicit.context_size = Some(32_768);

        assert_eq!(cfg.to_proxy_config().default_context, Some(32_768));
    }

    /// `GlobalDefaults::default` is defined *as* `ProxyConfig::default`, so a
    /// bare config must round-trip back to it. Guards against the two
    /// defaults drifting apart, which is exactly the class of bug this epic
    /// exists to kill.
    #[test]
    fn bare_globals_round_trip_to_proxy_config_defaults() {
        let derived = bare(GlobalDefaults::default()).to_proxy_config();
        let baseline = ProxyConfig::default();

        assert_eq!(derived.host, baseline.host);
        assert_eq!(derived.port, baseline.port);
        assert_eq!(derived.cache_enabled, baseline.cache_enabled);
        assert_eq!(derived.slot_dir, baseline.slot_dir);
        assert_eq!(derived.disk_budget, baseline.disk_budget);
        assert_eq!(derived.inference_override, baseline.inference_override);
    }

    #[test]
    fn proxy_config_slot_dir_matches_resolved_options() {
        let cfg = bare(GlobalDefaults {
            cache_enabled: true,
            slot_dir: Some(PathBuf::from("/custom/slots")),
            ..Default::default()
        });

        assert_eq!(
            cfg.to_proxy_config().slot_dir,
            cfg.resolved_options().slot_save_path
        );
    }
}
