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
    identifier: String,
    context: ContextArgs,
    options: ServeOptions,
    sampling: SamplingArgs,
    profile_flag: Option<String>,
    mtp: MtpArgs,
    cache: CacheArgs,
    access: AccessArgs,
    verbose: bool,
) -> Result<()> {
    // Ensure llama.cpp is installed before the daemon needs it.
    ensure_llama_initialized(&CliPrompt::new()).await?;

    let settings = ctx.app.settings().get().await?;

    // Resolve `--profile` or a `{model}:{profile}` suffix before model lookup:
    // the suffix names a profile, not a model, and must not reach the catalog.
    let selection = super::profile_selection::select(
        ctx.catalog.as_ref(),
        settings.inference_profiles.as_deref().unwrap_or_default(),
        &identifier,
        profile_flag.as_deref(),
    )
    .await?;

    let model = ctx
        .app
        .models()
        .find_by_identifier(&selection.model)
        .await
        .map_err(|e| {
            // Absence and failure read differently: a missing model is the
            // user's typo, a repository error is not, and telling someone to
            // run `gglib model list` when the pool is down wastes their time.
            anyhow::anyhow!(
                "could not resolve model '{}': {e}. If it is missing, \
                 'gglib model list' shows what is available.",
                selection.model
            )
        })?;

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
            inference_params: Some(sampling.clone().into_inference_config()),
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
            // The live path. gglib emits no sampler flags to llama-server
            // (ADR 0003/0004), so a value that does not travel as a
            // proxy-wide override does not reach the model at all — which is
            // exactly what used to happen to every `serve` sampling flag.
            inference_override: sampling.clone().into_override(),
        },
    );
    // Only what was passed, not the full resolution. The resolution printed
    // here is the five-rung stored ladder; what a request actually gets is the
    // proxy's six-rung one, which additionally has the client's own rung and
    // the agentic ceiling above it. Printing the former as if it were the
    // latter states values no request is guaranteed to see.
    let stated_sampling = sampling.into_override();
    let mtp_args = plan.mtp.clone();
    let effective_ctx = plan.effective_ctx;

    style::print_info_banner("Info", "\u{2139}\u{fe0f}");
    eprintln!("  Using model: {} (ID: {})", model.name, model.id);
    eprintln!("  File: {}", model.file_path.display());
    eprintln!("  Context size: {} (resolved)", effective_ctx);
    log_mlock_info(context.mlock);
    if let Some(ref stated) = stated_sampling {
        log_inference_info(stated);
        warn_client_budgets_are_capped(stated);
    }
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
            default_context: proxy_config.default_context,
            cache: Some(cache.cache),
            // `slot_dir` comes from the cascade rather than the raw flag: it
            // has already had the `cache_enabled` master switch applied and
            // the default directory filled in.
            slot_dir: proxy_config.slot_dir,
            pinned: Some(plan.pinned),
            cache_disk_gb: cache.cache_disk_gb,
            // Sampling rides the proxy-wide override, not the pinned model's
            // launch options. The comment that used to stand here claimed the
            // opposite and was wrong in a way nothing caught: the launch
            // options write to `ServerConfig::inference_config`, which is
            // documented as read by nobody, so every `serve` sampling flag was
            // resolved, printed, and discarded.
            inference_override: proxy_config.inference_override.clone(),
            // The profile is carried by name: the proxy re-reads its list per
            // request, so an edit takes effect without restarting this endpoint.
            default_profile: selection.profile.as_ref().map(|p| p.name.clone()),
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

/// Say so when a `serve` flag will override a client's own token budget.
///
/// `max_tokens` and `reasoning_budget_tokens` are the two values gglib lets an
/// untrusted client keep — the trust gate drops everything else it sends but
/// deliberately honours its budgets, because a budget states what the *client*
/// can afford rather than how the model should sample. An operator flag rides
/// the rung above, so naming either one here silently caps every client on
/// this endpoint, including the BYOK clients `serve` exists for.
///
/// Not a refusal: it is the operator's server and `cli_override` is exactly
/// the rung that says so. But it is a consequence worth stating to the person
/// who chose it, and unlike the coupling rule this needs no resolution
/// preview — it reads only which flags were typed, so it cannot be wrong.
fn warn_client_budgets_are_capped(stated: &gglib_core::domain::InferenceConfig) {
    let capped: Vec<&str> = [
        stated.max_tokens.map(|_| "--max-tokens"),
        stated
            .reasoning_budget_tokens
            .map(|_| "--reasoning-budget-tokens"),
    ]
    .into_iter()
    .flatten()
    .collect();

    if !capped.is_empty() {
        eprintln!(
            "  Note: {} overrides each client's own budget on this endpoint.",
            capped.join(" and ")
        );
    }
}
