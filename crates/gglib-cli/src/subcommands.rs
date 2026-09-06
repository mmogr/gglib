//! Nested subcommand enums for the commands that have them.
//!
//! `chat`, `proxy` and `daemon` each carry a second level of subcommands
//! (`gglib chat history`, `gglib proxy stop`, `gglib daemon run`). They live
//! here rather than beside [`Commands`](crate::commands::Commands) because
//! that enum sits on the 300 LOC ratchet, and a nested enum is the part of it
//! with no coupling to the top level — nothing here names a flag group, so the
//! split costs no shared context.
//!
//! Re-exported from [`crate::commands`], so callers keep naming them
//! `crate::commands::ChatCommand` and the move is invisible to dispatch.

use clap::Subcommand;

/// Subcommands available under `gglib chat`.
#[derive(Subcommand)]
pub enum ChatCommand {
    /// List past conversations (use `--continue <ID>` to resume one)
    History {
        /// Maximum number of conversations to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },
}

/// Subcommands available under `gglib proxy`.
#[derive(Subcommand)]
pub enum ProxyCommand {
    /// Show a live terminal dashboard for an already-running proxy
    ///
    /// Connects to `GET /v1/proxy/status/stream` on the target proxy and
    /// redraws active connections, llama.cpp `/slots` context usage, and
    /// the running request count in place until Ctrl+C is pressed.
    Dashboard {
        /// Host of the already-running proxy to connect to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port of the already-running proxy to connect to
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// API key of the target proxy, if it requires one.
        /// Omit to use the stored `proxy_api_key` setting.
        #[arg(long, env = "GGLIB_API_KEY")]
        api_key: Option<String>,
    },
    /// Clear KV cache on an already-running proxy
    ///
    /// Without `--session-id` this clears every disk slot *and* recycles
    /// llama-server, which is the only way to drop its host-RAM prompt cache
    /// (`--cache-ram`). The recycle is skipped while a request is in flight.
    ///
    /// With `--session-id` only that session's disk slots are cleared; the
    /// shared RAM cache is left alone.
    CacheClear {
        /// Host of the already-running proxy to connect to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port of the already-running proxy to connect to
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Optional session ID to target (without --session-id, clears all sessions)
        #[arg(long)]
        session_id: Option<String>,
        /// API key of the target proxy, if it requires one.
        /// Omit to use the stored `proxy_api_key` setting.
        #[arg(long, env = "GGLIB_API_KEY")]
        api_key: Option<String>,
    },
    /// Stop the proxy on the running gglib daemon
    ///
    /// The proxy keeps running in the daemon after `gglib proxy`/`gglib serve`
    /// detach; this is how it is actually stopped.
    Stop,
}

/// Subcommands available under `gglib daemon`.
#[derive(Subcommand)]
pub enum DaemonCommand {
    /// Run the daemon in the foreground
    ///
    /// The daemon is the one process on this machine that spawns and owns
    /// llama-server. Other commands start it automatically when they need it;
    /// run it manually for a foreground session or a service manager unit.
    Run {
        /// Expose the daemon on all LAN interfaces (0.0.0.0) and advertise
        /// it over mDNS
        ///
        /// The management API then requires a bearer token (the stored
        /// API key, minted and printed at startup if none exists). It can
        /// still start and stop inference on this machine — only use on
        /// networks you trust.
        #[arg(long)]
        share_lan: bool,

        /// Accept this Host header value in addition to loopback and IP
        /// literals (repeatable)
        ///
        /// The daemon refuses requests whose Host names a hostname it was
        /// never told about — the DNS-rebinding guard. Name your DNS alias
        /// here if you reach a shared daemon through one.
        #[arg(long = "allowed-host", value_name = "HOST")]
        allowed_host: Vec<String>,
    },
    /// Show whether the daemon is running, and what it is doing
    Status,
    /// Stop the running daemon (and every llama-server it owns)
    Stop,
}

/// Subcommands available under `gglib remote`.
///
/// Both sides of ADR 0012. Off by default and never persisted: `enable` arms
/// the serve side and `connect` the connect side for the running daemon only.
#[derive(Subcommand)]
pub enum RemoteCommand {
    /// Put this machine's proxy on another machine, and show the pairing
    ///
    /// Brings the tunnel up on the daemon, mints a fresh ticket and a
    /// six-digit pairing code, and shows both once. The code lives two
    /// minutes and is spent on first use. Enabling also puts the API key on
    /// the local proxy, and disabling does not take that away.
    Enable {
        /// Let requests arriving through the tunnel reach /mcp
        ///
        /// Off by default: invoke_tool starts the MCP servers configured on
        /// this machine, and a leaked key with a shell server configured is
        /// remote code execution.
        #[arg(long)]
        allow_mcp: bool,
        /// Self-hosted relay URL (default: iroh's public relays)
        #[arg(long)]
        relay: Option<String>,
        /// Do not publish to, or resolve through, n0's discovery service
        ///
        /// The ticket then carries only the paths it was minted with and
        /// stops working if this machine changes network.
        #[arg(long)]
        no_discovery: bool,
        /// Print the pairing as plain text instead of the QR screen
        #[arg(long)]
        no_qr: bool,
    },
    /// Take the tunnel down; the ticket is dead from that moment
    Disable,
    /// Show the tunnel's state, its peers, and what came through it
    Status,
    /// Reach another machine's proxy: bind a local port here that is it
    ///
    /// First time, paste the whole `<ticket>-<code>` string that machine's
    /// `gglib remote enable` showed; the code is redeemed through the tunnel
    /// for that machine's API key, which is stored here. Afterwards the
    /// ticket alone will do, and with no argument the last ticket is dialled.
    Connect {
        /// `<ticket>-<code>`, a bare ticket, or nothing to reuse the last one
        pairing: Option<String>,
        /// Loopback port to bind here (default: a free one)
        #[arg(long)]
        port: Option<u16>,
        /// Self-hosted relay URL for this side (default: iroh's public relays)
        #[arg(long)]
        relay: Option<String>,
        /// Do not resolve through n0's discovery service; dial only the
        /// paths the ticket carries
        #[arg(long)]
        no_discovery: bool,
    },
    /// Close the local port; the far machine and the stored pairing stay
    Disconnect,
    /// Stop the far machine's daemon through the tunnel, then disconnect
    ///
    /// A one-way door: nothing brings that daemon back except someone at
    /// the machine. Asks before doing it unless --yes is given.
    Kill {
        /// Do not ask for confirmation
        #[arg(long, short = 'y')]
        yes: bool,
    },
}
