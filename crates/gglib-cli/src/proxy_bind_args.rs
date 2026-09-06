//! Where `gglib proxy` binds, as a flag group.
//!
//! Three flags that used to sit inline on `Commands::Proxy`. They moved when
//! `gglib remote` joined the top-level enum, which is on the file-size
//! ratchet — the same reason `subcommands.rs` exists — and a bind is a group
//! of its own the way `SamplingArgs` and `CacheArgs` already are.

use clap::Args;

/// Host, port and default context for `gglib proxy`.
#[derive(Args, Debug, Clone)]
pub struct ProxyBindArgs {
    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    /// Port to bind the proxy to
    #[arg(short, long, default_value = "8080")]
    pub port: u16,
    /// Default context size when not specified by client.
    /// Falls back to the app settings `default_context_size`; with neither
    /// set, each launch is sized by the daemon — fitted to this machine
    /// where gglib can read the device, and the built-in floor where it
    /// cannot. Must be within the range
    /// `gglib config settings set --default-context-size` accepts — `max`
    /// is not supported here since no specific model is in scope for a
    /// standalone proxy.
    #[arg(long)]
    pub default_context: Option<String>,
}
