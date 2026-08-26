//! Tests for the proxy wire types.
//!
//! A `#[path]` sibling per the repo convention: `wire.rs` sat 12 lines under
//! the 300-line budget with a new contract test due.

use super::*;
use gglib_core::contracts::http::daemon::{PROXY_START_CLI_FIELDS, PROXY_START_DAEMON_ONLY_FIELDS};

/// An omitted port must come from settings, not from the compile-time
/// default. The tray panel sends no port at all, and starting it on 8080
/// while every other surface used the configured port is exactly the
/// split-brain this endpoint has to avoid.
#[test]
fn an_omitted_port_comes_from_settings() {
    let settings = AppSettings {
        proxy_port: Some(18080),
        ..AppSettings::default()
    };
    let runtime_cfg = to_runtime_config(&StartProxyConfig::default(), &settings);
    assert_eq!(runtime_cfg.port, 18080);
}

/// An explicit port still wins: settings are the fallback, not an override.
#[test]
fn an_explicit_port_beats_the_setting() {
    let settings = AppSettings {
        proxy_port: Some(18080),
        ..AppSettings::default()
    };
    let cfg = StartProxyConfig {
        port: Some(9999),
        ..Default::default()
    };
    assert_eq!(to_runtime_config(&cfg, &settings).port, 9999);
}

/// With neither a request port nor a saved one, the hard-coded default is
/// still the floor.
#[test]
fn no_port_anywhere_falls_back_to_the_default() {
    let runtime_cfg = to_runtime_config(&StartProxyConfig::default(), &AppSettings::default());
    assert_eq!(runtime_cfg.port, DEFAULT_PROXY_PORT);
}

/// Omitting `cache` must mean disabled, matching the CLI's own default —
/// and, with cache off, no slot dir even if one were supplied.
#[test]
fn cache_omitted_defaults_to_disabled() {
    let cfg = StartProxyConfig::default();
    let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
    assert!(!runtime_cfg.cache_enabled);
    assert_eq!(runtime_cfg.slot_dir, None);
}

/// The master switch beats an explicit slot dir, matching
/// `ProxyCacheOptions`/`UnifiedServerConfig` on the CLI side: cache off
/// means zero cache-related settings reach the runtime, full stop.
#[test]
fn cache_false_ignores_a_supplied_slot_dir() {
    let cfg = StartProxyConfig {
        cache: Some(false),
        slot_dir: Some(std::path::PathBuf::from("/custom/slots")),
        ..Default::default()
    };
    let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
    assert!(!runtime_cfg.cache_enabled);
    assert_eq!(runtime_cfg.slot_dir, None);
}

/// `cache: true` with an explicit directory must carry it through
/// unchanged — this is what lets a GUI-started proxy persist KV slots
/// anywhere but the default location.
#[test]
fn cache_true_carries_the_explicit_slot_dir() {
    let cfg = StartProxyConfig {
        cache: Some(true),
        slot_dir: Some(std::path::PathBuf::from("/custom/slots")),
        ..Default::default()
    };
    let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
    assert!(runtime_cfg.cache_enabled);
    assert_eq!(
        runtime_cfg.slot_dir,
        Some(std::path::PathBuf::from("/custom/slots"))
    );
}

/// `cache: true` with no directory must fall back to the same default the
/// CLI uses, not `None` — the Axum proxy path errors requests with
/// "slot_dir not configured" when cache is on and slot_dir is absent, so
/// leaving it `None` here would make `cache: true` alone insufficient.
#[test]
fn cache_true_without_slot_dir_uses_the_default_directory() {
    let cfg = StartProxyConfig {
        cache: Some(true),
        ..Default::default()
    };
    let runtime_cfg = to_runtime_config(&cfg, &AppSettings::default());
    assert!(runtime_cfg.cache_enabled);
    assert_eq!(
        runtime_cfg.slot_dir,
        Some(gglib_runtime::default_slot_dir())
    );
}

/// `default_context` resolution is untouched by the cache wiring — still
/// falls through explicit → settings → nothing, the floor being collapsed to
/// `None` so the launch can reach the fitted rung.
#[test]
fn default_context_falls_through_to_settings() {
    let cfg = StartProxyConfig::default();
    let settings = AppSettings {
        default_context_size: Some(16_384),
        ..AppSettings::default()
    };
    let runtime_cfg = to_runtime_config(&cfg, &settings);
    assert_eq!(runtime_cfg.default_context, Some(16_384));
}

/// A crashed proxy must not look reachable. It reports stopped, with no
/// port for a client to be handed and no pin left standing.
#[test]
fn a_crashed_proxy_reports_as_stopped() {
    let status = to_api_status(RuntimeProxyStatus::Crashed, Some("qwen".to_owned()));

    assert!(!status.running);
    assert_eq!(status.port, None);
    assert_eq!(status.pinned_model, None);
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire parity with the CLI
// ─────────────────────────────────────────────────────────────────────────────

/// A plausible JSON value for each name in the shared contract.
///
/// The catch-all panics rather than defaulting. A name added to
/// `PROXY_START_CLI_FIELDS` with no sample here stops the suite and says which
/// field; a `Value::Null` fallback would deserialize into `None` and let the
/// test pass while covering one field fewer than it claims.
fn sample_for(name: &str) -> serde_json::Value {
    use serde_json::json;
    match name {
        "host" => json!("127.0.0.1"),
        "port" => json!(8080),
        "llama_base_port" => json!(9100),
        "default_context" => json!(4096),
        "cache" => json!(true),
        "slot_dir" => json!("/slots"),
        "pinned" => serde_json::to_value(gglib_core::ports::PinnedSpec::default())
            .expect("PinnedSpec serialises"),
        "cache_disk_gb" => json!(8),
        "inference_override" => {
            serde_json::to_value(gglib_core::domain::InferenceConfig::default())
                .expect("InferenceConfig serialises")
        }
        "default_profile" => json!("fast"),
        "api_key" => json!("k"),
        "allowed_hosts" => json!(["example.test"]),
        other => panic!("no sample value for contract field `{other}` — add one"),
    }
}

/// Every field the CLI sends must survive into `StartProxyConfig`.
///
/// The destructuring is exhaustive on purpose, and must stay a destructuring
/// rather than a struct literal: `handlers/proxy/mod.rs` already builds one of
/// these with `..body.proxy`, and struct-update syntax never breaks when a
/// field is added.
#[test]
fn the_daemon_accepts_every_field_the_cli_sends() {
    let body: serde_json::Map<String, serde_json::Value> = PROXY_START_CLI_FIELDS
        .iter()
        .chain(PROXY_START_DAEMON_ONLY_FIELDS)
        .map(|name| ((*name).to_owned(), sample_for(name)))
        .collect();

    let cfg: StartProxyConfig = serde_json::from_value(serde_json::Value::Object(body))
        .expect("the daemon must read every field the CLI sends");

    let StartProxyConfig {
        host,
        port,
        llama_base_port,
        default_context,
        cache,
        slot_dir,
        pinned,
        cache_disk_gb,
        inference_override,
        default_profile,
        api_key,
        allowed_hosts,
    } = cfg;

    // The width is half the guard. `StartProxyConfig` has no
    // `deny_unknown_fields`, so a name in the shared list that the daemon never
    // grew a field for deserializes into nothing at all and leaves no assertion
    // to fail. Tying this array's length to the contract's total catches that
    // direction; the `E0027` above catches the other.
    let fields: [(&str, bool); 12] = [
        ("host", host.is_some()),
        ("port", port.is_some()),
        ("llama_base_port", llama_base_port.is_some()),
        ("default_context", default_context.is_some()),
        ("cache", cache.is_some()),
        ("slot_dir", slot_dir.is_some()),
        ("pinned", pinned.is_some()),
        ("cache_disk_gb", cache_disk_gb.is_some()),
        ("inference_override", inference_override.is_some()),
        ("default_profile", default_profile.is_some()),
        ("api_key", api_key.is_some()),
        ("allowed_hosts", !allowed_hosts.is_empty()),
    ];

    assert_eq!(
        fields.len(),
        PROXY_START_CLI_FIELDS.len() + PROXY_START_DAEMON_ONLY_FIELDS.len(),
        "the rows above have drifted from the shared contract — add or remove a row, \
         and its contract name, so the two agree"
    );

    for (name, survived) in fields {
        assert!(survived, "{name} was dropped");
    }
}

/// `gglib up` sends `{}` plus whatever it set — never an `allowed_hosts` key,
/// because the CLI skips it when empty. Removing `#[serde(default)]` from that
/// field would 422 those calls rather than degrade them.
#[test]
fn a_body_with_no_allowed_hosts_still_deserializes() {
    let cfg: StartProxyConfig = serde_json::from_str("{}").expect("an empty body must deserialize");
    assert!(cfg.allowed_hosts.is_empty());
}
