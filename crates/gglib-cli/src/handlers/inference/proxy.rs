//! Proxy command handler.
//!
//! `gglib proxy` runs [`start_proxy_standalone`](start_proxy_standalone)
//! unpinned — the counterpart to [`serve`](super::serve), which runs the same
//! entry point pinned to a single model. Subcommands that connect to an
//! already-running proxy (`dashboard`, `cache-clear`) are routed by the
//! dispatcher before reaching this module; this handler only starts one.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::shared_args::{CacheArgs, SamplingArgs};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
use gglib_runtime::proxy::{StandaloneProxyParams, start_proxy_standalone};

/// Execute the proxy command.
///
/// Starts the proxy unpinned — serving the whole catalog and auto-swapping
/// on request — and blocks until Ctrl-C.
pub async fn execute(
    ctx: &CliContext,
    host: String,
    port: u16,
    llama_port: u16,
    default_context: Option<String>,
    sampling: SamplingArgs,
    cache: CacheArgs,
) -> Result<()> {
    let settings = ctx.app.settings().get().await?;
    let effective_context = resolve_context_size(&ServerConfigOptions {
        context_size: default_context
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok()),
        global_default_ctx: settings.default_context_size,
        ..Default::default()
    });
    let inference_override = sampling.into_override();

    start_proxy_standalone(StandaloneProxyParams {
        host,
        port,
        llama_base_port: llama_port,
        llama_server_path: ctx.llama_server_path.clone(),
        model_repo: ctx.model_repo.clone(),
        mcp: ctx.mcp.clone(),
        settings_repo: ctx.app.settings().repo(),
        default_context: effective_context,
        inference_override,
        cache: cache.into_proxy_cache_options(),
        // `gglib proxy` serves the whole catalog and swaps on demand;
        // `gglib serve` is the pinned mode of this same entry point.
        pinned: None,
    })
    .await
}
