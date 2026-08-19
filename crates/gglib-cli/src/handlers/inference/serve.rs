//! Serve command handler.
//!
//! `gglib serve <model>` starts the daemon's proxy pinned to a single model.
//! It is not a separate way to launch llama-server — the flags are resolved
//! locally through the full `UnifiedServerConfig` cascade, and the result
//! travels to the daemon as the `pinned` field of `POST /api/proxy/start`.
//!
//! Pinning exists for clients that cannot switch models via `/v1/models` —
//! VS Code Copilot's BYOK endpoint being the motivating case. Requests naming
//! any other model are refused rather than silently swapped.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, StartProxyBody};
use crate::presentation::style;
use crate::shared_args::{AccessArgs, CacheArgs, ContextArgs, MtpArgs, SamplingArgs, ServeOptions};
use gglib_app_services::launch_options::{ProxyGlobals, plan_pinned_launch};
use gglib_app_services::types::StartServerRequest;
use gglib_core::server_config::parse_ctx_size_flag;
use gglib_runtime::llama::{CliPrompt, ensure_llama_initialized};

use super::shared::{log_inference_info, log_mlock_info};

/// Execute the serve command.
///
/// Resolves the model's launch options locally (the full cascade), asks the
/// daemon to start the proxy pinned to it, and attaches the dashboard.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    ctx: &CliContext,
    id: u32,
    context: ContextArgs,
    options: ServeOptions,
    sampling: SamplingArgs,
    mtp: MtpArgs,
    cache: CacheArgs,
    access: AccessArgs,
    verbose: bool,
) -> Result<()> {
    // Ensure llama.cpp is installed before the daemon needs it.
    ensure_llama_initialized(&CliPrompt::new()).await?;

    let model = ctx
        .app
        .models()
        .get_by_id(i64::from(id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("Model with ID {} not found", id))?;

    let settings = ctx.app.settings().get().await?;

    // The raw `--ctx-size` flag is shape-validated at parse time, before the
    // model is known; resolving it here against the model's GGUF context
    // length is what makes `--ctx-size max` work.
    let ctx_arg = parse_ctx_size_flag(context.ctx_size.as_deref())?;

    // The cascade itself is shared with the GUI's pinned start
    // (`gglib_app_services::launch_options`), so the two surfaces cannot
    // drift; this handler only maps flags into the shared request shape and
    // prints the banners from the resolved plan.
    let plan = plan_pinned_launch(
        &model,
        &settings,
        &StartServerRequest {
            context_length: ctx_arg.and_then(|arg| arg.resolve(model.context_length)),
            port: None,
            jinja: options.jinja.then_some(true),
            reasoning_format: None,
            mtp_draft_n_max: mtp.mtp_draft_n_max,
            mtp_draft_p_min: mtp.mtp_draft_p_min,
            inference_params: Some(sampling.into_inference_config()),
            mlock: context.mlock,
        },
        ProxyGlobals {
            host: Some(options.host.clone()),
            default_ctx: None,
            proxy_port: Some(options.port),
            // The daemon owns llama-server port allocation; a per-run value
            // has nothing to attach to. `config settings set --llama-base-port`.
            llama_base_port: None,
            cache_enabled: cache.cache,
            slot_dir: cache.slot_dir.clone(),
            api_key: access.api_key.clone(),
            allowed_hosts: access.allowed_hosts.clone(),
        },
    );
    let inference_config = plan.inference.clone();
    let mtp_args = plan.mtp.clone();
    let effective_ctx = plan.effective_ctx;

    style::print_info_banner("Info", "\u{2139}\u{fe0f}");
    eprintln!("  Using model: {} (ID: {})", model.name, model.id);
    eprintln!("  File: {}", model.file_path.display());
    eprintln!("  Context size: {} (resolved)", effective_ctx);
    log_mlock_info(context.mlock);
    log_inference_info(&inference_config);
    if options.jinja {
        eprintln!("  Jinja templates: enabled");
    }
    if mtp_args.enabled {
        eprintln!(
            "  MTP speculative decoding: enabled (n-max={}, p-min={:.2}, source={:?})",
            mtp_args.draft_n_max, mtp_args.draft_p_min, mtp_args.source
        );
    }
    style::print_banner_close();

    if verbose {
        tracing::debug!(
            model = %model.name,
            proxy_port = options.port,
            "starting pinned proxy on the daemon"
        );
    }

    let proxy_config = plan.unified.to_proxy_config();

    let handle = daemon_client::ensure_daemon().await?;
    let status = handle
        .start_proxy(&StartProxyBody {
            host: Some(proxy_config.host),
            port: Some(proxy_config.port),
            default_context: Some(proxy_config.default_context),
            cache: Some(cache.cache),
            // `slot_dir` comes from the cascade rather than the raw flag: it
            // has already had the `cache_enabled` master switch applied and
            // the default directory filled in.
            slot_dir: proxy_config.slot_dir,
            // Sampling rides the pinned model's launch options rather than a
            // proxy-wide override, so it lands on the llama-server command
            // line exactly as every other launch surface applies it.
            pinned: Some(plan.pinned),
            cache_disk_gb: cache.cache_disk_gb,
            inference_override: None,
            api_key: proxy_config.api_key,
            allowed_hosts: proxy_config.allowed_hosts,
        })
        .await?;

    let proxy_port = status.port.unwrap_or(options.port);
    super::proxy::attach_dashboard(ctx, proxy_port, access.api_key).await
}

#[cfg(test)]
#[path = "serve_tests.rs"]
mod serve_tests;
