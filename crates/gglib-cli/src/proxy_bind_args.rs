//! Where `gglib proxy` binds, as a flag group.
//!
//! Three flags of `Commands::Proxy`, grouped here because the top-level enum
//! is on the file-size ratchet — the same reason `subcommands.rs` exists —
//! and a bind is a group of its own the way `SamplingArgs` and `CacheArgs`
//! are.

use clap::Args;

/// Host, port and default context size for `gglib proxy`.
#[derive(Args, Debug, Clone)]
pub struct ProxyBindArgs {
    /// Host to bind to. Omit to use the stored `proxy_host` setting
    /// (loopback until one is stored)
    #[arg(long)]
    pub host: Option<String>,
    /// Port to bind the proxy to. Omit to use the stored `proxy_port` setting
    #[arg(short, long)]
    pub port: Option<u16>,
    /// Default context size when not specified by client.
    /// Falls back to the `default-context-size` setting; with neither
    /// set, each launch is sized by the daemon — fitted to this machine
    /// where gglib can read the device, and the built-in floor where it
    /// cannot. Must be within the range
    /// `gglib config set default-context-size` accepts — `max`
    /// is not supported here since no specific model is in scope for a
    /// standalone proxy.
    #[arg(long = "default-context-size", value_name = "N")]
    pub default_context: Option<String>,
}
