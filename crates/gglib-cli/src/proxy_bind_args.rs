//! Where `gglib proxy` binds, as a flag group.
//!
//! Three flags of `Commands::Proxy`, grouped here because a bind is a group
//! of its own the way `SamplingArgs` and `CacheArgs` are.

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
    /// set, each launch is sized by the daemon — fitted to this machine's
    /// GPU, or to its RAM when it has no GPU, and the built-in floor where
    /// a GPU's memory cannot be read. Must be within the range
    /// `gglib config set default-context-size` accepts — `max`
    /// is not supported here since no specific model is in scope for a
    /// standalone proxy.
    #[arg(long = "default-context-size", value_name = "N")]
    pub default_context: Option<String>,
    /// Switch the endpoint without asking when it is on otherwise.
    ///
    /// With the endpoint on serving one model, or with another port,
    /// address, sampling or cache than these flags ask, `proxy` asks before
    /// switching it, because a switch cuts off the clients connected now.
    /// `-y` answers yes. Without it, a run that is not in a terminal changes
    /// nothing.
    #[arg(short = 'y', long)]
    pub yes: bool,
}
