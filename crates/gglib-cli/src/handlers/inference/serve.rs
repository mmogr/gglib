//! Serve command handler.
//!
//! `gglib serve <model>` runs the unified proxy stack pinned to a single
//! model. It is not a separate way to launch llama-server — it is
//! [`start_proxy_standalone`](gglib_runtime::proxy::start_proxy_standalone)
//! with [`StandaloneProxyParams::pinned`] set, which is what gives it the
//! dashboard, KV cache lifecycle, SSE progress, request normalization and
//! upstream health monitoring that the old direct-spawn path lacked entirely.
//!
//! Pinning exists for clients that cannot switch models via `/v1/models` —
//! VS Code Copilot's BYOK endpoint being the motivating case. Requests naming
//! any other model are refused rather than silently swapped.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::presentation::style;
use crate::shared_args::{CacheArgs, ContextArgs, MtpArgs, SamplingArgs, ServeOptions};
use gglib_core::server_config::{ServerConfigOptions, parse_ctx_size_flag, resolve_context_size};
use gglib_runtime::llama::{ensure_llama_initialized, resolve_llama_server, resolve_mtp_args};
use gglib_runtime::proxy::{
    PinnedModel, ProxyCacheOptions, StandaloneProxyParams, start_proxy_standalone,
};
use gglib_runtime::unified_server_config::{GlobalDefaults, UnifiedServerConfig};

use super::shared::{log_inference_info, log_mlock_info, resolve_inference_config};

/// Execute the serve command.
///
/// Starts the proxy pinned to the requested model and blocks until Ctrl-C.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    ctx: &CliContext,
    id: u32,
    context: ContextArgs,
    options: ServeOptions,
    sampling: SamplingArgs,
    mtp: MtpArgs,
    cache: CacheArgs,
    verbose: bool,
) -> Result<()> {
    // Ensure llama.cpp is installed
    ensure_llama_initialized().await?;

    let llama_server_path = resolve_llama_server().map_err(|e| {
        anyhow::anyhow!(
            "{}\n\nTo install llama.cpp, run:\n  gglib config llama install",
            e
        )
    })?;

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

    let inference_config =
        resolve_inference_config(ctx, sampling.into_inference_config(), &model).await?;

    let mtp_args = resolve_mtp_args(mtp.mtp_draft_n_max, mtp.mtp_draft_p_min, &model.tags);

    // Everything the CLI knows, expressed as the three tiers. The cascade and
    // the translation into llama-server flags both happen downstream, so this
    // handler never assembles a command line of its own.
    let unified = UnifiedServerConfig {
        explicit: ServerConfigOptions {
            context_size: ctx_arg.and_then(|arg| arg.resolve(model.context_length)),
            model_server_ctx: model
                .server_defaults
                .as_ref()
                .and_then(|s| s.context_length),
            // `false` is the flag's absence, not a request to disable mlock —
            // leaving it None lets a future global default apply.
            mlock: context.mlock.then_some(true),
            jinja: options.jinja.then_some(true),
            inference_params: Some(inference_config.clone()),
            mtp_draft_n_max: mtp_args.enabled.then_some(mtp_args.draft_n_max),
            mtp_draft_p_min: mtp_args.enabled.then_some(mtp_args.draft_p_min),
            ..Default::default()
        },
        // The cache master switch and directory are tier 3: `resolved_options`
        // applies `cache_enabled` over `slot_save_path`, which is how
        // `--cache` reaches llama-server on the pinned model's launch options
        // at all. The remaining cache flags carry no model-specific meaning
        // and go straight to the proxy via `ProxyCacheOptions` — including
        // `--cache-disk-gb`, which `start_proxy_standalone` resolves into a
        // `DiskBudget` for both commands alike.
        globals: GlobalDefaults {
            host: options.host.clone(),
            proxy_port: options.port,
            llama_base_port: options.llama_port,
            default_ctx: settings.default_context_size,
            cache_enabled: cache.cache,
            slot_dir: cache.slot_dir.clone(),
            ..Default::default()
        },
    };

    let launch_overrides = unified.resolved_options();
    let effective_ctx = resolve_context_size(&launch_overrides);

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
            llama_port = options.llama_port,
            "starting pinned proxy"
        );
    }

    let proxy_config = unified.to_proxy_config();

    start_proxy_standalone(StandaloneProxyParams {
        host: proxy_config.host,
        port: proxy_config.port,
        llama_base_port: unified.globals.llama_base_port,
        llama_server_path,
        model_repo: ctx.model_repo.clone(),
        mcp: ctx.mcp.clone(),
        settings_repo: ctx.app.settings().repo(),
        default_context: proxy_config.default_context,
        // Sampling is carried on the pinned model's launch options rather than
        // as a proxy-wide override, so it lands on the llama-server command
        // line exactly as every other launch surface applies it.
        inference_override: None,
        // `slot_dir` comes from the cascade rather than the raw flag: it has
        // already had the `cache_enabled` master switch applied and the
        // default directory filled in.
        cache: ProxyCacheOptions {
            slot_dir: proxy_config.slot_dir,
            ..cache.into_proxy_cache_options()
        },
        pinned: Some(PinnedModel {
            id: model.id,
            name: model.name.clone(),
            launch_overrides,
        }),
    })
    .await
}

#[cfg(test)]
mod tests {
    use crate::shared_args::CacheArgs;
    use gglib_core::cache_config::KvCacheType;
    use gglib_core::server_config::{
        CtxSizeArg, ServerConfigOptions, parse_ctx_size_flag, resolve_context_size,
    };
    use gglib_runtime::unified_server_config::{GlobalDefaults, UnifiedServerConfig};
    use std::path::PathBuf;

    /// Mirrors how `execute` assembles its config, minus the I/O.
    fn unified(explicit: ServerConfigOptions, globals: GlobalDefaults) -> UnifiedServerConfig {
        UnifiedServerConfig { explicit, globals }
    }

    /// `--ctx-size max` resolves against the model's GGUF context length,
    /// which is only known after the model is fetched — hence the deferred
    /// parse the handler performs.
    #[test]
    fn ctx_size_max_resolves_against_model_metadata() {
        let arg = parse_ctx_size_flag(Some("max")).unwrap();
        assert_eq!(arg, Some(CtxSizeArg::Max));

        let cfg = unified(
            ServerConfigOptions {
                context_size: arg.and_then(|a| a.resolve(Some(131_072))),
                ..Default::default()
            },
            GlobalDefaults::default(),
        );

        assert_eq!(resolve_context_size(&cfg.resolved_options()), 131_072);
    }

    /// An omitted `--ctx-size` must fall through the cascade rather than
    /// pinning the context to a hardcoded value.
    #[test]
    fn omitted_ctx_size_falls_through_to_the_global_default() {
        let cfg = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                default_ctx: Some(8192),
                ..Default::default()
            },
        );

        assert_eq!(resolve_context_size(&cfg.resolved_options()), 8192);
    }

    /// An absent `--mlock` must stay `None`, not `Some(false)`: the flag's
    /// absence is "no opinion", which lets lower tiers still apply.
    #[test]
    fn absent_mlock_flag_expresses_no_opinion() {
        let cfg = unified(
            ServerConfigOptions {
                mlock: false.then_some(true),
                ..Default::default()
            },
            GlobalDefaults::default(),
        );

        assert_eq!(cfg.resolved_options().mlock, None);
    }

    #[test]
    fn mlock_flag_reaches_the_resolved_options() {
        let cfg = unified(
            ServerConfigOptions {
                mlock: true.then_some(true),
                ..Default::default()
            },
            GlobalDefaults::default(),
        );

        assert_eq!(cfg.resolved_options().mlock, Some(true));
    }

    /// `--port` is the proxy's listener and `--llama-port` the upstream. They
    /// must stay distinct or the proxy would try to bind the port its own
    /// llama-server is on.
    #[test]
    fn proxy_and_llama_ports_are_carried_separately() {
        let cfg = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                proxy_port: 8080,
                llama_base_port: 5500,
                ..Default::default()
            },
        );

        assert_eq!(cfg.to_proxy_config().port, 8080);
        assert_eq!(cfg.globals.llama_base_port, 5500);
    }

    /// Serve binds loopback by default — the security gap that motivated
    /// routing this command through the proxy stack in the first place.
    #[test]
    fn serve_binds_loopback_by_default() {
        let cfg = unified(ServerConfigOptions::default(), GlobalDefaults::default());
        assert_eq!(cfg.to_proxy_config().host, "127.0.0.1");
    }

    /// `--host` must reach the proxy config — otherwise there is no way to
    /// serve a pinned endpoint to another machine on a trusted network.
    #[test]
    fn explicit_host_overrides_the_loopback_default() {
        let cfg = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                host: "0.0.0.0".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(cfg.to_proxy_config().host, "0.0.0.0");
    }

    // ---------------------------------------------------------------
    // Cache flags (#633 — parity with `gglib proxy`)
    // ---------------------------------------------------------------

    /// Without `--cache`, no `--slot-save-path` may reach llama-server even
    /// when a directory was named: the master switch outranks the directory,
    /// so "cache off" means byte-for-byte no cache flags.
    #[test]
    fn slot_dir_without_cache_flag_emits_no_slot_path() {
        let cfg = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                cache_enabled: false,
                slot_dir: Some(PathBuf::from("/custom/slots")),
                ..Default::default()
            },
        );

        assert_eq!(cfg.resolved_options().slot_save_path, None);
        assert_eq!(cfg.to_proxy_config().slot_dir, None);
    }

    /// `--cache --slot-dir` must reach the pinned model's launch options —
    /// the path by which disk KV-slot persistence works on `serve` at all.
    #[test]
    fn cache_flag_carries_the_slot_dir_into_launch_options() {
        let cfg = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                cache_enabled: true,
                slot_dir: Some(PathBuf::from("/custom/slots")),
                ..Default::default()
            },
        );

        assert_eq!(
            cfg.resolved_options().slot_save_path,
            Some(PathBuf::from("/custom/slots"))
        );
    }

    /// `--cache` with no directory falls back to the default rather than
    /// silently disabling persistence.
    #[test]
    fn cache_flag_without_slot_dir_uses_the_default_directory() {
        let cfg = unified(
            ServerConfigOptions::default(),
            GlobalDefaults {
                cache_enabled: true,
                ..Default::default()
            },
        );

        assert!(cfg.resolved_options().slot_save_path.is_some());
    }

    /// The model-independent cache flags ride `ProxyCacheOptions` rather than
    /// the cascade, and must survive the conversion intact.
    #[test]
    fn model_independent_cache_flags_reach_the_proxy_options() {
        let opts = CacheArgs {
            cache: true,
            cache_ram_mb: Some(4096),
            cache_reuse: Some(256),
            cache_disk_gb: Some(8),
            cache_type_k: Some(KvCacheType::F16),
            cache_type_v: Some(KvCacheType::Q8_0),
            ..Default::default()
        }
        .into_proxy_cache_options();

        assert!(opts.enabled);
        assert_eq!(opts.ram_mb, Some(4096));
        assert_eq!(opts.reuse, Some(256));
        assert_eq!(opts.disk_gb, Some(8));
        assert_eq!(opts.type_k, Some(KvCacheType::F16));
        assert_eq!(opts.type_v, Some(KvCacheType::Q8_0));
    }
}
