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
mod tests {
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
}
