//! The pinned-launch cascade, shared by every surface that pins the proxy.
//!
//! `gglib serve` and the GUI's pinned start must produce byte-identical
//! launch options for the same inputs — both call [`plan_pinned_launch`];
//! the CLI keeps its banners by reading the plan's resolved fields rather
//! than recomputing them. [`crate::ServerOps`]'s bare-start builder remains
//! a deliberate sibling: its explicit tier passes raw values through because
//! the spawn path re-gates them per launch.

use std::path::PathBuf;

use gglib_core::domain::{InferenceConfig, ModelSamplingContext};
use gglib_core::ports::PinnedSpec;
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_core::{Model, Settings};
use gglib_runtime::llama::{MtpResolution, resolve_mtp_args};
use gglib_runtime::unified_server_config::{GlobalDefaults, UnifiedServerConfig};

use crate::types::StartServerRequest;

/// Proxy-process inputs for a pinned launch — the tier-3 half of the cascade.
///
/// Every field is "no opinion" when unset; [`GlobalDefaults`]' hardened
/// defaults (loopback bind, default ports) apply underneath.
#[derive(Debug, Clone, Default)]
pub struct ProxyGlobals {
    pub host: Option<String>,
    pub proxy_port: Option<u16>,
    pub llama_base_port: Option<u16>,
    /// Caller-supplied fallback context — overrides the settings default on
    /// the cascade's third rung. The CLI passes `None` (its `serve` has no
    /// such flag); the GUI start-pinned body may carry one.
    pub default_ctx: Option<u64>,
    /// Master switch for disk KV-slot persistence.
    pub cache_enabled: bool,
    pub slot_dir: Option<PathBuf>,
    pub api_key: Option<String>,
    pub allowed_hosts: Vec<String>,
}

/// Everything a pinned start needs, resolved once.
#[derive(Debug, Clone)]
pub struct PinnedLaunch {
    /// The two cascade tiers, kept whole so callers can derive the proxy
    /// config (`to_proxy_config`) without re-assembly.
    pub unified: UnifiedServerConfig,
    /// The pin itself: model name + fully resolved launch options.
    pub pinned: PinnedSpec,
    /// Sampling after the full merge hierarchy — what the CLI banner prints.
    pub inference: InferenceConfig,
    /// MTP resolution (explicit overrides layered over model tags).
    pub mtp: MtpResolution,
    /// The context size the model will actually get.
    pub effective_ctx: u64,
}

/// Resolve a pinned launch from a model, settings, and per-call overrides.
///
/// `request.context_length` is already numeric — the CLI resolves its
/// `--ctx-size max` affordance against the model before calling this.
/// `request.mlock == false` means "no opinion", never "disable": the flag's
/// absence must leave lower tiers free to apply.
#[must_use]
pub fn plan_pinned_launch(
    model: &Model,
    settings: &Settings,
    request: &StartServerRequest,
    globals: ProxyGlobals,
) -> PinnedLaunch {
    let model_ctx = ModelSamplingContext::for_model(model);
    let inference = request
        .inference_params
        .clone()
        .unwrap_or_default()
        .resolve_with_defaults(
            model.inference_defaults.as_ref(),
            settings.inference_defaults.as_ref(),
            model_ctx,
        );

    let mtp = resolve_mtp_args(
        request.mtp_draft_n_max,
        request.mtp_draft_p_min,
        &model.tags,
    );

    let mut tier3 = GlobalDefaults {
        default_ctx: globals.default_ctx.or(settings.default_context_size),
        cache_enabled: globals.cache_enabled,
        slot_dir: globals.slot_dir,
        api_key: globals.api_key,
        allowed_hosts: globals.allowed_hosts,
        ..Default::default()
    };
    if let Some(host) = globals.host {
        tier3.host = host;
    }
    if let Some(port) = globals.proxy_port {
        tier3.proxy_port = port;
    }
    if let Some(port) = globals.llama_base_port {
        tier3.llama_base_port = port;
    }

    let unified = UnifiedServerConfig {
        explicit: ServerConfigOptions {
            context_size: request.context_length,
            port: request.port,
            model_server_ctx: model
                .server_defaults
                .as_ref()
                .and_then(|s| s.context_length),
            mlock: request.mlock.then_some(true),
            jinja: request.jinja,
            reasoning_format: request.reasoning_format.clone(),
            inference_params: Some(inference.clone()),
            mtp_draft_n_max: mtp.enabled.then_some(mtp.draft_n_max),
            mtp_draft_p_min: mtp.enabled.then_some(mtp.draft_p_min),
            ..Default::default()
        },
        globals: tier3,
    };

    let launch_overrides = unified.resolved_options();
    let effective_ctx = resolve_context_size(&launch_overrides);

    PinnedLaunch {
        pinned: PinnedSpec {
            name: model.name.clone(),
            launch_overrides,
        },
        unified,
        inference,
        mtp,
        effective_ctx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        Model {
            dialect_spec: None,
            id: 1,
            name: "test-model".to_string(),
            model_key: String::new(),
            file_path: std::path::PathBuf::from("/tmp/model.gguf"),
            param_count_b: 7.0,
            architecture: None,
            quantization: None,
            context_length: Some(131_072),
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: std::collections::HashMap::new(),
            added_at: chrono::Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: vec![],
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
            capabilities: gglib_core::domain::capabilities::ModelCapabilities::default(),
            benchmark_summary: None,
        }
    }

    fn request() -> StartServerRequest {
        StartServerRequest::default()
    }

    /// The explicit tier outranks the model's server defaults, which outrank
    /// the settings default — the same precedence `gglib serve` documents.
    #[test]
    fn context_resolves_explicit_over_model_over_settings() {
        let mut m = model();
        m.server_defaults = Some(gglib_core::domain::ServerConfig {
            context_length: Some(16_384),
        });
        let settings = Settings {
            default_context_size: Some(8192),
            ..Default::default()
        };

        let explicit = plan_pinned_launch(
            &m,
            &settings,
            &StartServerRequest {
                context_length: Some(4096),
                ..request()
            },
            ProxyGlobals::default(),
        );
        assert_eq!(explicit.effective_ctx, 4096);

        let model_tier = plan_pinned_launch(&m, &settings, &request(), ProxyGlobals::default());
        assert_eq!(model_tier.effective_ctx, 16_384);

        m.server_defaults = None;
        let settings_tier = plan_pinned_launch(&m, &settings, &request(), ProxyGlobals::default());
        assert_eq!(settings_tier.effective_ctx, 8192);
    }

    /// `mlock: false` is the flag's absence, not a request to disable — the
    /// invariant the CLI's `--mlock` handling depends on.
    #[test]
    fn absent_mlock_expresses_no_opinion() {
        let plan = plan_pinned_launch(
            &model(),
            &Settings::default(),
            &request(),
            ProxyGlobals::default(),
        );
        assert_eq!(plan.pinned.launch_overrides.mlock, None);
    }

    /// Resolved sampling must land on the pin's launch options — sampling
    /// rides the pinned model, never a proxy-wide override.
    #[test]
    fn resolved_sampling_reaches_the_pin() {
        let plan = plan_pinned_launch(
            &model(),
            &Settings::default(),
            &StartServerRequest {
                inference_params: Some(InferenceConfig {
                    temperature: Some(0.4),
                    ..Default::default()
                }),
                ..request()
            },
            ProxyGlobals::default(),
        );
        let params = plan
            .pinned
            .launch_overrides
            .inference_params
            .as_ref()
            .unwrap();
        assert_eq!(params.temperature, Some(0.4));
    }

    /// The cache master switch outranks the directory: cache off means no
    /// slot path reaches llama-server, byte-for-byte.
    #[test]
    fn cache_master_switch_gates_the_slot_dir() {
        let off = plan_pinned_launch(
            &model(),
            &Settings::default(),
            &request(),
            ProxyGlobals {
                cache_enabled: false,
                slot_dir: Some(PathBuf::from("/custom/slots")),
                ..Default::default()
            },
        );
        assert_eq!(off.pinned.launch_overrides.slot_save_path, None);

        let on = plan_pinned_launch(
            &model(),
            &Settings::default(),
            &request(),
            ProxyGlobals {
                cache_enabled: true,
                slot_dir: Some(PathBuf::from("/custom/slots")),
                ..Default::default()
            },
        );
        assert_eq!(
            on.pinned.launch_overrides.slot_save_path,
            Some(PathBuf::from("/custom/slots"))
        );
    }

    /// The request's llama port must reach the pin's launch options — the
    /// ServeModal's Port field on the pinned path (review-gate blocker).
    #[test]
    fn request_port_reaches_the_pin() {
        let plan = plan_pinned_launch(
            &model(),
            &Settings::default(),
            &StartServerRequest {
                port: Some(9345),
                ..request()
            },
            ProxyGlobals::default(),
        );
        assert_eq!(plan.pinned.launch_overrides.port, Some(9345));
    }

    /// A caller-supplied fallback context overrides the settings default on
    /// the third rung without touching the explicit tier.
    #[test]
    fn caller_default_ctx_overrides_the_settings_rung() {
        let settings = Settings {
            default_context_size: Some(8192),
            ..Default::default()
        };
        let plan = plan_pinned_launch(
            &model(),
            &settings,
            &request(),
            ProxyGlobals {
                default_ctx: Some(16_384),
                ..Default::default()
            },
        );
        assert_eq!(plan.effective_ctx, 16_384);
    }

    /// Unset proxy globals keep the hardened defaults — loopback bind above all.
    #[test]
    fn default_globals_bind_loopback() {
        let plan = plan_pinned_launch(
            &model(),
            &Settings::default(),
            &request(),
            ProxyGlobals::default(),
        );
        assert_eq!(plan.unified.to_proxy_config().host, "127.0.0.1");
    }
}
