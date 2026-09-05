//! Which machine answers: the flags `chat` and `question` share for choosing
//! their upstream.

use clap::Args;

/// Where the agent loop sends its completions.
///
/// Three states, one struct: nothing set asks the daemon to start the model
/// here; `--port` reuses a llama-server already running here; `--remote`
/// drives the machine on the other end of `gglib remote connect`. The last
/// two are exclusive because they name different machines.
#[derive(Args, Debug, Clone, Default)]
pub struct UpstreamArgs {
    /// Reuse an already-running llama-server on this port (skips auto-start)
    #[arg(long, conflicts_with = "remote")]
    pub port: Option<u16>,

    /// Ask the machine connected with `gglib remote connect` instead of this one
    ///
    /// The daemon must be connected and hold the key from that pairing; the
    /// model is whatever the far machine serves, so a model name here is
    /// forwarded rather than looked up.
    #[arg(long)]
    pub remote: bool,
}
