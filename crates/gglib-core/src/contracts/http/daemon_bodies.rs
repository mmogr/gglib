//! The keys of the request bodies the CLI sends to `gglib daemon`.
//!
//! The two ends of such a body cannot meet in one test. The CLI's struct is
//! `pub(crate)` inside `gglib-cli`'s `pub(crate) mod daemon_client`, and the
//! daemon's is `pub(crate)` inside `gglib-axum`'s `pub(crate) mod handlers`;
//! both crates deny `unreachable_pub`, and gglib-axum may not depend on
//! gglib-cli. So each side pins itself against the list here instead — the
//! same trick [`super::daemon::CLI_ROUTE_CONTRACT`] uses for paths.

/// Every key the CLI puts in a `POST /api/proxy/start` body: `StartProxyBody`
/// there, `StartProxyConfig` in the daemon.
pub const PROXY_START_CLI_FIELDS: &[&str] = &[
    "host",
    "port",
    "default_context",
    "cache",
    "slot_dir",
    "pinned",
    "cache_disk_gb",
    "inference_override",
    "default_profile",
    "api_key",
    "allowed_hosts",
];

/// Keys the daemon accepts on that body which the CLI never sends.
///
/// `llama_base_port` is read only by `POST /api/proxy/start-pinned`, which
/// routes it through the launch cascade. `/api/proxy/start` deserializes it and
/// never looks at it, so it is daemon-only by function rather than by omission.
pub const PROXY_START_DAEMON_ONLY_FIELDS: &[&str] = &["llama_base_port"];

/// Every key the CLI puts in a `POST /api/servers/start` body: `StartServerBody`
/// at both ends, each an id beside a flattened `StartServerRequest`.
///
/// The daemon's body refuses no unknown key, so a key spelled another way is
/// dropped without an error, and the setting it carried with it.
pub const SERVERS_START_CLI_FIELDS: &[&str] = &[
    "id",
    "contextLength",
    "port",
    "jinja",
    "reasoningFormat",
    "mtpDraftNMax",
    "mtpDraftPMin",
    "inferenceParams",
    "mlock",
];
