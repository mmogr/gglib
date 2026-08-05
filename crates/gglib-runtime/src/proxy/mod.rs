#![doc = include_str!("README.md")]
// `pub(crate)` for the launch narration: the banner is the proxy's output
// voice, but the launch it narrates happens in `process::swap_state`.
pub(crate) mod banner;
pub mod models;
pub mod params;
pub mod supervisor;

// Re-export supervisor types
use params::compose_launch_overrides;
pub use params::{PinnedModel, ProxyAccessOptions, ProxyCacheOptions, StandaloneProxyParams};
pub use supervisor::{ProxyBind, ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};

use anyhow::{Result, anyhow};
use std::sync::Arc;

use crate::ports_impl::{CatalogPortImpl, RuntimePortImpl};
use crate::process::ProcessManager;
use gglib_core::ports::ModelCatalogPort;
use gglib_core::server_config::CacheRamSetting;

// =============================================================================
// start_proxy_standalone
// =============================================================================

/// Start the OpenAI-compatible proxy as a standalone server (CLI usage).
///
/// The single entry point behind both `gglib proxy` and `gglib serve`. It
/// creates every component internally and blocks until shutdown.
///
/// [`StandaloneProxyParams::pinned`] is what separates the two commands: when
/// set, the process manager refuses every model but the pinned one, giving
/// single-model clients a fixed endpoint. The Axum layer, cache lifecycle,
/// dashboard, SSE, MCP gateway and shutdown path are shared verbatim between
/// the modes — `serve` is a mode of the proxy, not a second stack.
///
/// # Errors
///
/// Returns an error if the HTTP client cannot be built, the proxy cannot bind
/// its address, or shutdown fails.
pub async fn start_proxy_standalone(params: StandaloneProxyParams) -> Result<()> {
    let StandaloneProxyParams {
        host,
        port,
        llama_base_port,
        llama_server_path,
        model_repo,
        mcp,
        settings_repo,
        default_context,
        inference_override,
        cache,
        access,
        pinned,
    } = params;

    // Resolve the actual KV cache slot-save directory. `None` whenever the
    // feature is disabled, whatever directory was passed — this is what makes
    // `--cache` off mean zero cache-related flags downstream.
    let slot_save_path = cache.resolved_slot_dir();
    let launch_overrides =
        compose_launch_overrides(&cache, pinned.as_ref(), slot_save_path.clone());

    // Create catalog port from model repository
    let catalog_port: Arc<dyn ModelCatalogPort> =
        Arc::new(CatalogPortImpl::new(Arc::clone(&model_repo)));

    // One manager either way — pinned only changes which models it admits.
    // No explicit cache-RAM value means auto-size rather than "leave the
    // llama-server default": the proxy is the one launch surface where a
    // right-sized prompt cache is the whole point.
    let cache_ram = cache
        .ram_mb
        .map_or(CacheRamSetting::Auto, CacheRamSetting::ExplicitMb);
    let llama_path = llama_server_path.to_string_lossy().into_owned();

    let process_manager = Arc::new(match &pinned {
        Some(model) => ProcessManager::new_pinned(
            model.name.clone(),
            llama_base_port,
            llama_path,
            Arc::clone(&catalog_port),
            launch_overrides,
            cache_ram,
        ),
        None => ProcessManager::new_single_swap(
            llama_base_port,
            llama_path,
            Arc::clone(&catalog_port),
            launch_overrides,
            cache_ram,
        ),
    });

    // Create runtime port
    let runtime_port: Arc<dyn gglib_core::ports::ModelRuntimePort> =
        Arc::new(RuntimePortImpl::new(Arc::clone(&process_manager)));

    // Create supervisor
    let supervisor = ProxySupervisor::new();

    // Start proxy
    let config = ProxyConfig {
        host: host.clone(),
        port,
        default_context,
        cache_enabled: cache.enabled,
        slot_dir: slot_save_path,
        disk_budget: gglib_proxy::slot_eviction::resolve_disk_budget(cache.disk_gb),
        // Passed as its own top-priority sampling layer rather than folded into
        // the persisted global defaults, which sit below the per-model layer.
        inference_override: inference_override.clone(),
        api_key: access.api_key.clone(),
        allowed_hosts: access.allowed_hosts.clone(),
    };

    // Initialize MCP service (validates servers and auto-starts enabled ones)
    if let Err(e) = mcp.initialize().await {
        tracing::warn!("MCP initialization completed with errors: {e}");
    }

    // Gather MCP counts for banner
    let servers = mcp.list_servers().await.unwrap_or_default();
    let eager_count = servers
        .iter()
        .filter(|s| s.lifecycle == gglib_core::McpLifecycle::Eager)
        .count();
    let lazy_count = servers
        .iter()
        .filter(|s| s.lifecycle == gglib_core::McpLifecycle::Lazy)
        .count();
    let manual_count = servers
        .iter()
        .filter(|s| s.lifecycle == gglib_core::McpLifecycle::Manual)
        .count();
    let tools = mcp.list_all_tools().await;
    let tool_count: usize = tools.iter().map(|(_, v)| v.len()).sum();

    banner::print_starting(
        pinned.as_ref(),
        &host,
        port,
        llama_base_port,
        default_context,
        inference_override.as_ref(),
        cache.enabled,
        config.slot_dir.as_deref(),
        servers.len(),
        eager_count,
        lazy_count,
        manual_count,
        tool_count,
    );

    let bind = supervisor
        .start(config, runtime_port, catalog_port, mcp, settings_repo)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    tracing::info!("Proxy started on {}", bind.addr);

    banner::print_ready(
        bind.addr,
        pinned.as_ref(),
        bind.api_key.as_deref(),
        bind.api_key_source,
    );

    // Wait for Ctrl-C
    tokio::signal::ctrl_c().await?;

    // Show shutdown message
    println!();
    println!("  Shutting down proxy...");

    // Stop proxy
    supervisor.stop().await.map_err(|e| anyhow!("{e}"))?;

    println!("  Proxy stopped");
    println!();

    Ok(())
}
