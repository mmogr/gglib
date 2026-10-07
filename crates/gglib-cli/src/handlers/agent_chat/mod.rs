#![doc = include_str!("README.md")]
pub(crate) mod config;
pub(crate) mod drain;
pub(crate) mod images;
mod markdown;
mod memory_jogger;
pub(crate) mod persistence;
#[allow(
    clippy::needless_pass_by_value,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod renderer;
pub(crate) mod repl;
mod repl_line;
pub(crate) mod resume_settings;
pub(crate) mod sampling_warning;
pub(crate) mod sight;
mod thinking_dispatch;
mod tool_format;
pub(crate) mod upstream;

use anyhow::{Result, bail};

use gglib_core::domain::agent::{AgentMessage, saved_history};
use gglib_core::domain::chat::ConversationSettings;

use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;

use self::images::TurnImages;
use self::persistence::Conversation;
use self::sight::Sight;

/// Entry point: start the interactive agentic REPL.
///
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
/// When `args.continue_id` is set, loads a previous conversation and resumes
/// with the original session parameters (saved settings fill in any CLI args
/// the user didn't explicitly provide).
#[allow(
    clippy::default_trait_access,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    // A file that cannot be attached ends the command before a conversation
    // is made or a model asked for.
    let (attachments, mut receipts) = (ctx.app.attachments(), std::io::stderr());
    let mut images = TurnImages::attach(attachments, &args.images, false, &mut receipts).await?;
    let Session {
        args,
        params,
        persistence,
        prior_messages,
    } = prepare(ctx, args).await?;
    let sight = Sight::of_session(ctx, &params).await;
    images.judge(sight, &prior_messages).await?;

    // 2. Compose the agent with the (possibly merged) args.
    let inference_config = args.sampling.clone().into_inference_config();
    let sampling = if inference_config == Default::default() {
        None
    } else {
        Some(inference_config)
    };
    let prior_chars: usize = prior_messages
        .iter()
        .map(gglib_core::AgentMessage::char_count)
        .sum();
    let banner = config::BannerInfo {
        quiet: false,
        sampling: sampling.clone(),
        prior_history_chars: if prior_chars > 0 {
            Some(prior_chars)
        } else {
            None
        },
    };
    let agent = config::compose(ctx, &params, None, sampling, &banner).await?;

    // The llama-server belongs to the daemon and stays warm for the next
    // session; nothing to stop here.
    repl::run_repl_with_prior(agent, &args, persistence, prior_messages, images).await
}

/// A session ready to compose: the merged args, the parameters its agent is
/// composed with, the conversation it saves to, and the messages it resumes.
struct Session<'a> {
    args: ChatArgs,
    params: config::AgentSessionParams,
    persistence: Option<Conversation<'a>>,
    prior_messages: Vec<AgentMessage>,
}

/// Everything [`run`] does before it reaches the daemon: read the settings,
/// create or resume the conversation, and settle the model and the profile.
#[allow(
    clippy::useless_let_if_seq,
    reason = "grandfathered at lint inheritance, #1157"
)]
async fn prepare<'a>(ctx: &'a CliContext, args: &ChatArgs) -> Result<Session<'a>> {
    // 1. If resuming, load the conversation first and merge saved settings
    //    into args so the agent is composed with the correct parameters.
    let mut args = args.clone();

    // Resolve max_iterations and max_stagnation_steps from persisted settings
    // when not already provided (there is no per-run stagnation flag).
    if let Ok(settings) = ctx.app.settings().get().await {
        if args.max_iterations.is_none() {
            args.max_iterations = settings.max_tool_iterations.map(|v| v as usize);
        }
        if args.max_stagnation_steps.is_none() {
            args.max_stagnation_steps = settings.max_stagnation_steps.map(|v| v as usize);
        }
    }

    // Strip any `{model}:{profile}` suffix before a conversation is created:
    // what it names is persisted, and a stored suffix would come back on
    // every resume as a profile the user did not type this time — colliding
    // with their `--profile` and making the session unresumable. What that
    // means on the paired machine, whose profiles these are not, is
    // `profile_selection`'s to say.
    let profile_settings = ctx.app.settings().get().await?;
    let configured_profiles = profile_settings
        .inference_profiles
        .as_deref()
        .unwrap_or_default();
    let typed_this_invocation = !args.identifier.is_empty();
    // A conversation that stored its model resumes on that model's machine.
    let stored = persistence::continued(ctx.app.chat_history(), args.continue_id).await?;
    if let Some(conv) = &stored {
        let pairing = profile_settings.remote_pairing.as_ref();
        resume_settings::follow_stored_machine(&mut args, conv, pairing)?;
    }
    let mut selected_profile = None;
    if let Some(selection) = crate::handlers::inference::profile_selection::select_before_resume(
        args.target,
        ctx.catalog.as_ref(),
        configured_profiles,
        &args.identifier,
        args.profile.as_deref(),
        typed_this_invocation,
    )
    .await?
    {
        args.identifier = selection.model;
        selected_profile = selection.profile;
    }

    let (persistence, prior_messages, saved) = if let Some(conv) = stored {
        let (merged_args, conv, prior, saved) = resume_conversation(ctx, &args, conv).await?;
        args = merged_args;
        (Some(conv), prior, saved)
    } else {
        args.identifier = args
            .target
            .model_for_turn(ctx, std::mem::take(&mut args.identifier), async || {
                bail!("model identifier is required (use --continue <ID> to resume a session)")
            })
            .await?;
        (None, Vec::new(), None)
    };

    // On a resume the identifier came from storage, not from this command
    // line. An explicit `--profile` is therefore the only thing the user
    // actually typed, and it wins over any suffix an older conversation
    // recorded rather than colliding with it.
    if !typed_this_invocation {
        selected_profile = crate::handlers::inference::profile_selection::resume_profile(
            args.target,
            ctx.catalog.as_ref(),
            configured_profiles,
            &mut args.identifier,
            args.profile.as_deref(),
        )
        .await?;
    }
    // Then the profile the conversation was saved with, if nothing named one.
    let selected_profile = resume_settings::restore_profile(
        selected_profile,
        args.target,
        configured_profiles,
        saved.as_ref().and_then(|s| s.profile.as_deref()),
    );

    // Resolved once, on the machine that serves the turn, before anything
    // is saved: a far model that is not there is refused here. A resume on
    // another model than the conversation stored then stores that one.
    let turn = args
        .target
        .resolve_turn(ctx, std::mem::take(&mut args.identifier))
        .await?;
    args.identifier.clone_from(&turn.identifier);
    let profile = selected_profile.as_ref();
    let persistence = match persistence {
        Some(conv) => {
            let typed = typed_this_invocation;
            let kept = resume_settings::resumed_settings(saved, &args, typed, profile, &turn)?;
            Some(conv.record_settings(kept).await)
        }
        None => resume_settings::new_conversation(ctx, &args, profile, &turn).await,
    };

    let params = config::AgentSessionParams {
        model_identifier: args.identifier.clone(),
        profile: selected_profile,
        turn: Some(turn),
        ..config::AgentSessionParams::from(&args)
    };
    Ok(Session {
        args,
        params,
        persistence,
        prior_messages,
    })
}

/// Load a previous conversation, merge its saved settings into args, and prepare for resume.
///
/// Settings restoration follows the principle: **saved settings are defaults,
/// explicit CLI flags override**. For example:
/// ```text
/// gglib chat other-model --continue 42 --temperature 0.9
/// ```
/// uses `other-model` and temperature `0.9` from the CLI, but restores
/// everything else (system prompt, `top_p`, tools, etc.) from conversation 42.
/// Which machine it resumes on, and by which id, `prepare` has already taken
/// from the model the conversation stored. The saved settings come back too:
/// whether their profile applies is [`resume_settings::restore_profile`]'s
/// to decide, and what the resume saves in their place is
/// [`resume_settings::resumed_settings`]'s.
async fn resume_conversation<'a>(
    ctx: &'a CliContext,
    args: &ChatArgs,
    conv: gglib_core::domain::chat::Conversation,
) -> Result<(
    ChatArgs,
    Conversation<'a>,
    Vec<AgentMessage>,
    Option<ConversationSettings>,
)> {
    let history = ctx.app.chat_history();
    let conv_id = conv.id;

    let db_messages = history.get_messages(conv_id).await?;
    let msg_count = db_messages.len();

    if msg_count == 0 {
        println!("Conversation #{conv_id} has no messages — starting fresh.");
    } else {
        print!(
            "{}",
            memory_jogger::memory_jogger(&db_messages, &conv.title)
        );
    }

    // Merge saved settings into a copy of the current args.
    let merged = resume_settings::apply_saved_settings(args, &conv.system_prompt, &conv.settings);

    if merged.identifier.is_empty() {
        bail!(
            "cannot resume conversation #{conv_id}: no model name was saved and none was provided on the CLI"
        );
    }

    // The history as every surface reads it back. The system prompt is the
    // conversation's, unless this command line names another.
    let prior_messages = saved_history(merged.system_prompt.as_deref(), &db_messages);

    // All of it is saved already, or is the prompt, which is never a row.
    let persistence = Conversation::resume(history, conv_id, prior_messages.len()).await;
    Ok((merged, persistence, prior_messages, conv.settings))
}
