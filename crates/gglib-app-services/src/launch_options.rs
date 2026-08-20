//! The pinned-launch cascade, shared by every surface that pins the proxy.
//!
//! `gglib serve` and the GUI's pinned start must produce byte-identical
//! launch options for the same inputs — both call [`plan_pinned_launch`];
//! the CLI keeps its banners by reading the plan's resolved fields rather
//! than recomputing them. [`crate::ServerOps`]'s bare-start builder remains
//! a deliberate sibling: its explicit tier passes raw values through because
//! the spawn path re-gates them per launch.

use std::path::PathBuf;

use gglib_core::domain::{FieldSources, InferenceConfig, ModelSamplingContext};
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
    /// Sampling the operator stated on the command line, applied above every
    /// client's own request parameters.
    ///
    /// Reaches `SamplingLayers::cli_override` through the proxy config. It is
    /// deliberately *not* a launch flag: gglib emits no sampler flags at all
    /// (`llama::args::sampling::sampler_flags` is empty, per ADR 0003/0004),
    /// so a value that does not travel this way does not reach llama.cpp.
    pub inference_override: Option<InferenceConfig>,
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
    /// Which rung supplied each of those values.
    ///
    /// Carried so a caller can tell a flag that won from one the coupling rule
    /// discarded — the two are indistinguishable in `inference` alone.
    pub sources: FieldSources,
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
    let (inference, sources) = request
        .inference_params
        .clone()
        .unwrap_or_default()
        .resolve_with_profile_explained(
            None,
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
        inference_override: globals.inference_override,
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
        sources,
        mtp,
        effective_ctx,
    }
}

#[cfg(test)]
#[path = "launch_options_tests.rs"]
mod launch_options_tests;
