//! Which machine a command is for.
//!
//! ADR 0013. `--remote` is one flag, declared once on the root parser and
//! accepted after every subcommand, and this is the value it becomes. Every
//! question whose answer depends on which machine a turn runs on is asked
//! of a [`Target`] and answered here — which upstream, whether this
//! machine's catalogue applies, how the request pipeline shapes the turn,
//! what the model on the wire is called, and what a turn is for when
//! nobody named a model. A handler holds a `Target` and calls; it never
//! tests it.
//!
//! Before this the same flag was a `bool` threaded by hand through six
//! `if remote` branches under `handlers/agent_chat/`, each deciding a
//! different thing, and every remote-aware behaviour meant finding them
//! all. The branches are here now, one per question, so a machine a
//! command could run on is an arm in each rather than a search.
//!
//! What `--remote` reaches is [`reach`]'s table: a command either *uses* a
//! machine or is about this one, and a command that is about this one
//! refuses the flag with a sentence rather than ignoring it.

use anyhow::{Result, anyhow, bail};
use gglib_core::domain::Model;
use gglib_core::request_pipeline::{self, ModelContext};
use gglib_core::{RemotePairing, SettingsUpdate};
use gglib_runtime::FarMachine;

use crate::bootstrap::CliContext;
use crate::commands::{Commands, DaemonCommand, ProxyCommand};
use crate::handlers::agent_chat::config::{AgentSessionParams, BannerInfo};
use crate::handlers::agent_chat::upstream;
use crate::model_commands::ModelCommand;

#[path = "target_remote.rs"]
mod remote;
#[path = "target_use.rs"]
mod use_side;
use remote::remote_upstream;

/// The machine a command runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Target {
    /// This machine: its daemon, its catalogue, its profiles.
    #[default]
    Local,
    /// The machine on the other end of `gglib remote join` (ADR 0012):
    /// its proxy through the tunnel's loopback port, its catalogue, its
    /// profiles, and the key this machine received when it paired.
    Remote,
}

/// What `--remote` does to a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    /// The command is about this machine, and stays so.
    Local,
    /// The command is about this machine for a reason the general sentence
    /// would get wrong, so the refusal says this instead.
    LocalBecause(&'static str),
    /// The command uses a machine, and `--remote` says which.
    Use,
}

/// Which commands `--remote` reaches, and what each is called.
///
/// Exhaustive on purpose: a command added to [`Commands`] does not compile
/// until it has said which it is, and the safe answer is [`Reach::Local`] —
/// using the paired machine is opt-in per command, never inherited.
pub(crate) fn reach(command: &Commands) -> (&'static str, Reach) {
    match command {
        Commands::Chat { .. } => ("chat", Reach::Use),
        Commands::Question { .. } => ("q", Reach::Use),
        // Loading a model is using the machine; everything else under
        // `model` changes what is on it.
        Commands::Serve { .. } => ("serve", Reach::Use),
        Commands::Model { command } => match command {
            ModelCommand::List { .. } => ("model list", Reach::Use),
            _ => ("model", Reach::Local),
        },
        // Stopping the far daemon is the one door ADR 0012 opened on purpose
        // (its `remote kill`); it lives under `daemon` now, beside the local
        // stop, and asks the same question first.
        Commands::Daemon { command } => match command {
            DaemonCommand::Stop { .. } => ("daemon stop", Reach::Use),
            _ => ("daemon", Reach::Local),
        },
        // Reading the far proxy's dashboard and clearing its cache are its
        // own routes, already through the tunnel. Stopping it is not a thing
        // the tunnel can survive, and the sentence says what to run instead.
        Commands::Proxy { command, .. } => match command {
            Some(ProxyCommand::Dashboard { .. }) => ("proxy dashboard", Reach::Use),
            Some(ProxyCommand::CacheClear { .. }) => ("proxy cache-clear", Reach::Use),
            Some(ProxyCommand::Stop) => (
                "proxy stop",
                Reach::LocalBecause(
                    "the far proxy is what carries a remote request, so stopping it from here \
                     would cut the tunnel from under itself. `gglib daemon stop --remote` \
                     stops that machine, proxy and all, and asks first.",
                ),
            ),
            None => ("proxy", Reach::Local),
        },
        Commands::Up { .. } => ("up", Reach::Local),
        Commands::Config { .. } => ("config", Reach::Local),
        Commands::Mcp { .. } => ("mcp", Reach::Local),
        Commands::Benchmark { .. } => ("benchmark", Reach::Local),
        Commands::Gui { .. } => ("gui", Reach::Local),
        Commands::Web { .. } => ("web", Reach::Local),
        Commands::Remote { .. } => ("remote", Reach::Local),
        Commands::Completions { .. } => ("completions", Reach::Local),
    }
}

/// The commands `--remote` reaches, as the refusal names them. A list
/// rather than derived from [`reach`], so that the sentence a person reads
/// is written by a person and stays in the order they would say it.
const REACHES: &str = "chat, q, serve, model list, proxy dashboard, proxy cache-clear, daemon stop";

impl Target {
    pub(crate) const fn from_flag(remote: bool) -> Self {
        if remote { Self::Remote } else { Self::Local }
    }

    /// Refuse a command this target does not reach.
    ///
    /// # Errors
    ///
    /// The one sentence, naming the command and what `--remote` does reach.
    pub(crate) fn admit(self, command: &Commands) -> Result<()> {
        let (name, reach) = reach(command);
        match (self, reach) {
            (Self::Remote, Reach::Local) => bail!(
                "`gglib {name}` is about this machine, and --remote does not change that. \
                 What --remote reaches on the paired machine: {REACHES}."
            ),
            (Self::Remote, Reach::LocalBecause(why)) => bail!("`gglib {name}` --remote: {why}"),
            (Self::Local, _) | (Self::Remote, Reach::Use) => Ok(()),
        }
    }

    /// The model name that goes on the wire, from `--model` and the
    /// positional.
    ///
    /// Locally the positional names a catalogue entry and `compose` looks it
    /// up, so an absent `--model` correctly leaves the wire name empty and
    /// llama-server serves whatever it loaded. On the paired machine there
    /// is no catalogue here to resolve the positional against, so the
    /// positional *is* the wire name — and dropping it sends `""`, which the
    /// far proxy answers with `404 Model '' not found`.
    pub(crate) fn wire_model_name(self, model: Option<String>, identifier: &str) -> Option<String> {
        match self {
            Self::Local => model,
            Self::Remote => {
                model.or_else(|| (!identifier.is_empty()).then(|| identifier.to_owned()))
            }
        }
    }

    /// The model a turn is for, when the command line may have named none.
    ///
    /// `here` is what this machine does about an empty name — `q` looks up
    /// the default model, `chat` refuses — and runs only locally. On the
    /// paired machine the answer is the model this machine last asked it
    /// for, remembered against the pairing; and a name that *was* typed
    /// becomes that memory, so the next turn need not repeat it.
    ///
    /// # Errors
    ///
    /// Whatever `here` says; on the paired machine, no pairing, nothing
    /// remembered yet, or a settings write that failed.
    pub(crate) async fn model_for_turn(
        self,
        ctx: &CliContext,
        typed: String,
        here: impl AsyncFnOnce() -> Result<String>,
    ) -> Result<String> {
        match self {
            Self::Local if typed.is_empty() => here().await,
            Self::Local => Ok(typed),
            Self::Remote => remembered_model(ctx, typed).await,
        }
    }

    /// Where the completion adapter points, and with what.
    ///
    /// # Errors
    ///
    /// Locally, whatever starting or finding the llama-server says; on the
    /// paired machine, a daemon that is not running or not connected, or a
    /// pairing this machine holds no key for.
    pub(crate) async fn upstream(
        self,
        ctx: &CliContext,
        params: &AgentSessionParams,
        banner: &BannerInfo,
    ) -> Result<Upstream> {
        match self {
            Self::Local => {
                let port = upstream::resolve_port(ctx, params, banner).await?;
                Ok(Upstream {
                    base_url: format!("http://127.0.0.1:{port}"),
                    far_machine: None,
                })
            }
            Self::Remote => remote_upstream(ctx, banner).await,
        }
    }

    /// This machine's catalogue entry for `identifier`, when this machine's
    /// catalogue is the one that applies. The paired machine runs the
    /// sampling ladder over *its* models, and an entry here of the same
    /// name describes a different file.
    pub(crate) async fn local_model(self, ctx: &CliContext, identifier: &str) -> Option<Model> {
        match self {
            Self::Local => ctx.app.models().find_by_identifier(identifier).await.ok(),
            Self::Remote => None,
        }
    }

    /// How the request pipeline shapes a turn. The paired machine's models
    /// are not in this catalogue; passthrough lets its proxy shape the
    /// request, which it does for every client.
    pub(crate) async fn model_context(self, ctx: &CliContext, identifier: &str) -> ModelContext {
        match self {
            Self::Local => request_pipeline::resolve(ctx.catalog.as_ref(), Some(identifier)).await,
            Self::Remote => ModelContext::passthrough(),
        }
    }
}

/// Where the completion adapter points, and with what.
pub(crate) struct Upstream {
    /// `http://127.0.0.1:<port>`, without the `/v1` — the adapter adds it.
    pub base_url: String,
    /// The far machine on the remote path — its key and the fingerprint it
    /// is known by; nothing for a local server.
    pub far_machine: Option<FarMachine>,
}

/// The paired machine's model for this turn: `typed`, remembered for next
/// time; or what was remembered; or a refusal that says how to find one.
async fn remembered_model(ctx: &CliContext, typed: String) -> Result<String> {
    let settings = ctx
        .app
        .settings()
        .get()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?;
    let Some(pairing) = settings.remote_pairing else {
        bail!(
            "this machine has not paired with a remote — `gglib remote join <ticket>-<code>` \
             first"
        );
    };
    if typed.is_empty() {
        return pairing.default_model.ok_or_else(|| {
            anyhow!(
                "name a model the first time — `gglib model list` on that machine shows the ones \
                 it serves; after that, --remote remembers the one you used"
            )
        });
    }
    if pairing.default_model.as_deref() != Some(typed.as_str()) {
        ctx.app
            .settings()
            .update(SettingsUpdate {
                remote_pairing: Some(Some(RemotePairing {
                    default_model: Some(typed.clone()),
                    ..pairing
                })),
                ..SettingsUpdate::default()
            })
            .await
            .map_err(|e| anyhow!("could not remember the model for that machine: {e}"))?;
    }
    Ok(typed)
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod target_tests;
