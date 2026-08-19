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
