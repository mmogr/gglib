//! Maps [`AgentSessionParams`] to an [`AgentLoopPort`] composition root.
//! Which machine it talks to, and everything that follows from that, is
//! the [`Target`]'s decision (ADR 0013); a llama-server here is
//! [`super::upstream`]'s.
//!
//! The only public surface is [`compose`], which returns the ready-to-use
//! `Arc<dyn AgentLoopPort>`. The llama-server itself belongs to the daemon —
//! this process never spawns or stops one, and a model left warm after the
//! session is a feature, not a leak. [`AgentConfig`] is built inline by the
//! caller from the same args.
//!
//! [`AgentConfig`]: gglib_core::domain::agent::AgentConfig

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::AgentLoopPort;
use gglib_core::request_pipeline::SamplingLayers;
use gglib_runtime::compose_agent_loop_with_sampling;

use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;
use crate::target::{Target, TurnModel};

// =============================================================================
// Types
// =============================================================================

/// Minimal parameter set needed to compose an agent session.
///
/// Extracted from [`ChatArgs`] so that different callers (interactive chat,
/// single-turn question) can compose the agent loop without constructing a
/// full `ChatArgs`.
#[derive(Debug, Clone)]
pub(crate) struct AgentSessionParams {
    /// Model name or ID used to start llama-server.
    pub model_identifier: String,
    /// Optional context-size override (numeric string or `"max"`).
    pub ctx_size: Option<String>,
    /// When set, reuse an already-running llama-server instead of auto-starting.
    pub port: Option<u16>,
    /// Which machine the turn runs on (ADR 0013). `port` is consulted only
    /// on this one.
    pub target: Target,
    /// Tool allowlist (empty = all tools visible).
    pub tools: Vec<String>,
    /// Model-name override forwarded to llama-server routing.
    pub model_name: Option<String>,
    /// Budget for retrying transient upstream failures, already resolved from
    /// `--no-retry` and the `GGLIB_LLM_RETRY_*` overrides.
    pub retry_policy: gglib_core::retry::RetryPolicy,
    /// The selected sampling profile, already resolved from either
    /// `--profile` or a `{model}:{profile}` suffix.
    ///
    /// Resolved by the caller rather than here, because `model_identifier`
    /// must already have any suffix stripped by the time `resolve_port` asks
    /// the daemon to start it.
    pub profile: Option<gglib_core::domain::InferenceProfile>,
    /// The model as the machine serving the turn resolved it, once
    /// ([`Target::resolve_turn`]): what the banner names and the conversation
    /// stores.
    pub turn: Option<TurnModel>,
}

/// Display metadata for the server-startup info banner.
///
/// Callers populate this with whatever session context they have so that
/// `resolve_port` can render a richer startup message.
#[derive(Debug, Clone, Default)]
pub(crate) struct BannerInfo {
    /// Suppress the banner entirely (e.g. `gglib q -Q`).
    pub quiet: bool,
    /// Sampling overrides to display (only non-default values are shown).
    pub sampling: Option<InferenceConfig>,
    /// Character count of prior conversation history being loaded (resume only).
    pub prior_history_chars: Option<usize>,
}

impl From<&ChatArgs> for AgentSessionParams {
    fn from(args: &ChatArgs) -> Self {
        // When --no-tools is set, use a sentinel allowlist that matches nothing
        // so the agent loop exposes zero tools to the model.
        let tools = if args.no_tools {
            vec!["__none__".into()]
        } else {
            args.tools.clone()
        };
        Self {
            model_identifier: args.identifier.clone(),
            ctx_size: args.context.ctx_size.clone(),
            port: args.port,
            target: args.target,
            tools,
            model_name: args
                .target
                .wire_model_name(args.model.clone(), &args.identifier),
            retry_policy: args.retry_policy,
            profile: None,
            turn: None,
        }
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Compose the agent loop ready to use for a session.
///
/// The llama-server behind the returned agent is either the one the caller
/// pointed at (`--port`) or one the daemon started; either way its lifetime
/// is not this session's concern.
///
/// When `sandbox_root` is `Some`, filesystem tools are restricted to that
/// directory.  Pass `None` for an unsandboxed session.
pub(crate) async fn compose(
    ctx: &CliContext,
    params: &AgentSessionParams,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
    banner: &BannerInfo,
) -> Result<Arc<dyn AgentLoopPort>> {
    // 1. Resolve the upstream — a llama-server here (reused, or started by
    //    the daemon) or the paired machine's tunnel port.
    let upstream = params.target.upstream(ctx, params, banner).await?;

    // 2. Gather the stored sampling layers beneath the flags: the selected
    //    profile and this machine's global defaults, and with them its
    //    agentic sampling switch. They go to the adapter unfolded, beside
    //    the flags (`sampling`) and the model's own values (`model_context`,
    //    below): the ladder is folded once, where each request is shaped,
    //    and a value resolved here would reach that fold as one a person
    //    typed. When the identifier is unknown (external port reuse with no
    //    catalog entry) the flags are forwarded with nothing stored beneath
    //    them, the switch included, so the ceiling stays on — and the target
    //    says whether this catalog is the one that applies at all.
    let catalogued = params
        .target
        .local_model(ctx, &params.model_identifier)
        .await?
        .is_some();
    let layers = if catalogued {
        let settings = ctx.app.settings().get().await?;
        SamplingLayers {
            agentic_adjustments: settings.effective_agentic_sampling(),
            profile: params.profile.as_ref().map(|chosen| chosen.config.clone()),
            global: settings.inference_defaults,
            ..SamplingLayers::default()
        }
    } else {
        SamplingLayers {
            agentic_adjustments: true,
            ..SamplingLayers::default()
        }
    };
    // Only that fold knows whether it passed over a flag, so it says so
    // through this; never under `-Q`, which promises silence on stderr.
    let sampling_observer = sampling
        .clone()
        .filter(|_| catalogued && !banner.quiet)
        .map(super::sampling_warning::discarded_flags);

    // 3. Initialise MCP servers (CLI bootstrap intentionally skips this).
    //    A failure is logged as a warning rather than aborting the session:
    //    the agent can still run without tools.
    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }
    // Pre-warm lazy servers so they are ready before the first agent iteration.
    ctx.mcp.prewarm_lazy().await;

    // 4. Compose the agent loop.  When tools are specified the loop is
    //    restricted to the named allowlist; otherwise all MCP tools are visible.
    let tool_filter = if params.tools.is_empty() {
        None
    } else {
        Some(params.tools.iter().cloned().collect())
    };
    let model_context = params
        .target
        .model_context(ctx, &params.model_identifier)
        .await;
    let agent = compose_agent_loop_with_sampling(
        upstream.base_url,
        ctx.http_client.clone(),
        params.model_name.clone(),
        model_context,
        Arc::clone(&ctx.mcp),
        tool_filter,
        sandbox_root,
        sampling,
        layers,
        sampling_observer,
        // No proxy dashboard in the CLI process — nowhere to report reuse.
        None,
        // Nor anywhere to report the guard's decisions: the ledger the GUI
        // counts into belongs to the daemon's process, and `gglib chat` is not
        // in it (#1091).
        None,
        Some(params.retry_policy),
        upstream.far_machine,
        ctx.app.attachments().store(),
    );

    Ok(agent)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "config_sampling_tests.rs"]
mod sampling_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::test_context;
    use crate::target::target_tests::{UNREADABLE, with_unreadable_catalogue};

    /// A session on `--port` composes over a model this catalogue does not
    /// hold, its sampling sent as typed. Over a catalogue that cannot be read
    /// it does not compose: nobody could say whether the model has a row.
    #[tokio::test]
    async fn a_session_composes_over_a_missing_model_and_not_over_an_unreadable_catalogue() {
        let dir = tempfile::tempdir().expect("tempdir");
        let args = ChatArgs {
            identifier: "qwen3".into(),
            port: Some(9),
            ..chat_args()
        };
        let (params, banner) = (AgentSessionParams::from(&args), BannerInfo::default());

        let lacking = test_context(dir.path()).await;
        let composed = compose(&lacking, &params, None, None, &banner).await;
        assert!(composed.is_ok(), "{:?}", composed.err());

        let unreadable = with_unreadable_catalogue(&dir).await;
        let composed = compose(&unreadable, &params, None, None, &banner).await;
        let error = composed.err().expect("the read failed").to_string();
        assert_eq!(error, UNREADABLE);
    }

    /// A `ChatArgs` with every knob at rest, so each test states only the two
    /// or three fields it is actually about.
    pub(super) fn chat_args() -> ChatArgs {
        ChatArgs {
            identifier: String::new(),
            context: crate::shared_args::ContextArgs::default(),
            system_prompt: None,
            sampling: crate::shared_args::SamplingArgs::default(),
            retry_policy: gglib_core::retry::RetryPolicy::default(),
            no_tools: false,
            port: None,
            target: Target::Local,
            max_iterations: None,
            tools: Vec::new(),
            tool_timeout_ms: None,
            max_parallel: None,
            images: Vec::new(),
            verbose: false,
            model: None,
            profile: None,
            continue_id: None,
            thinking: None,
            observation_tools: Vec::new(),
            max_observation_steps: None,
        }
    }

    #[test]
    fn remote_forwards_the_positional_when_no_model_flag_was_given() {
        let args = ChatArgs {
            identifier: "qwen3".into(),
            target: Target::Remote,
            ..chat_args()
        };
        assert_eq!(
            AgentSessionParams::from(&args).model_name,
            Some("qwen3".into())
        );
    }

    #[test]
    fn remote_prefers_the_model_flag_over_the_positional() {
        let args = ChatArgs {
            identifier: "qwen3".into(),
            model: Some("llama3".into()),
            target: Target::Remote,
            ..chat_args()
        };
        assert_eq!(
            AgentSessionParams::from(&args).model_name,
            Some("llama3".into())
        );
    }

    #[test]
    fn locally_the_positional_is_not_the_wire_name() {
        // The catalog lookup in `compose` resolves the positional here, and an
        // absent `--model` still means "whatever llama-server loaded".
        let args = ChatArgs {
            identifier: "qwen3".into(),
            ..chat_args()
        };
        let params = AgentSessionParams::from(&args);
        assert_eq!(params.model_name, None);
        assert_eq!(params.model_identifier, "qwen3");
    }
}
