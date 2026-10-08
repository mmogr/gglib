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

use gglib_core::domain::agent::{AgentMessage, TurnLimits, saved_history};
use gglib_core::domain::chat::ConversationSettings;

use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::handlers::inference::chat::ChatArgs;

use self::images::TurnImages;
use self::persistence::Conversation;
use self::sight::Sight;

/// Entry point: start the interactive agentic REPL.
///
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
/// When `args.continue_id` is set, loads a previous conversation and resumes
/// with the original session parameters (saved settings fill in any CLI args
/// the user didn't explicitly provide), unless the daemon is replying to
/// that conversation ([`refuse_running_elsewhere`]).
#[allow(
    clippy::default_trait_access,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    // Before anything is stored: an image, a setting, a row.
    if let Some(id) = args.continue_id {
        refuse_running_elsewhere(ctx, id).await?;
    }
    // A file that cannot be attached ends the command before a conversation
    // is made or a model asked for.
    let (attachments, mut receipts) = (ctx.app.attachments(), std::io::stderr());
    let mut images = TurnImages::attach(attachments, &args.images, false, &mut receipts).await?;
    let Session {
        args,
        params,
        limits,
        persistence,
        prior_messages,
    } = prepare(ctx, args).await?;
    let sight = Sight::of_session(ctx, &params).await?;
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
    repl::run_repl_with_prior(agent, &args, limits, persistence, prior_messages, images).await
}

/// Refuse to continue chat `id` while the daemon is replying to it for the
/// page or a paired device, and so writing its rows: the rule the daemon
/// refuses a second run by ([`gglib_core::domain::runs::RunInfo::holds`]),
/// read off its own listing. Refused only on the daemon's word. With none
/// running no daemon run is replying; and one that does not list its runs for
/// this data root's token, as another data root's does not, has said nothing
/// of this chat.
async fn refuse_running_elsewhere(ctx: &CliContext, id: i64) -> Result<()> {
    if let Ok(daemon) = daemon_client::running(ctx).await
        && let Ok(listed) = daemon.run_list().await
        && let Some(run) = listed.runs.iter().find(|run| run.holds(id))
    {
        bail!(
            "chat {id} is running elsewhere: the daemon is still replying to it (run {run}). \
             Wait for the reply, or stop it with: gglib run cancel {run}",
            run = run.id
        );
    }
    Ok(())
}

/// A session ready to compose: the merged args, the parameters its agent is
/// composed with, the limits its turns run with, the conversation it saves
/// to, and the messages it resumes.
struct Session<'a> {
    args: ChatArgs,
    params: config::AgentSessionParams,
    limits: TurnLimits,
    persistence: Option<Conversation<'a>>,
    prior_messages: Vec<AgentMessage>,
}

/// What [`run`] does before it composes its agent: read the settings, create
/// or resume the conversation, and settle the model and the profile.
#[allow(
    clippy::useless_let_if_seq,
    reason = "grandfathered at lint inheritance, #1157"
)]
async fn prepare<'a>(ctx: &'a CliContext, args: &ChatArgs) -> Result<Session<'a>> {
    // 1. If resuming, load the conversation first and merge saved settings
    //    into args so the agent is composed with the correct parameters.
    let mut args = args.clone();

    // Strip any `{model}:{profile}` suffix before a conversation is created:
    // what it names is persisted, and a stored suffix would come back on
    // every resume as a profile the user did not type this time — colliding
    // with their `--profile` and making the session unresumable. What that
    // means on the paired machine, whose profiles these are not, is
    // `profile_selection`'s to say.
    let settings = ctx.app.settings().get().await?;
    let configured_profiles = settings.inference_profiles.as_deref().unwrap_or_default();
    let typed_this_invocation = !args.identifier.is_empty();
    // A conversation that stored its model resumes on that model's machine.
    let stored = persistence::continued(ctx.app.chat_history(), args.continue_id).await?;
    if let Some(conv) = &stored {
        let pairing = settings.remote_pairing.as_ref();
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

    let (resumed, prior_messages, saved) = if let Some(conv) = stored {
        let id = conv.id;
        let (merged_args, prior, saved) = resume_conversation(ctx, &args, conv).await?;
        args = merged_args;
        (Some(id), prior, saved)
    } else {
        args.identifier = args
            .target
            .model_for_turn(ctx, std::mem::take(&mut args.identifier), async || {
                bail!("model identifier is required (use --continue <ID> to resume a session)")
            })
            .await?;
        (None, Vec::new(), None)
    };
    // By the rule the daemon reads a turn's Thinking choice with: `--thinking`
    // wins and is remembered below, once there is a conversation to remember
    // it; without it the chat runs as it remembers.
    let remembered = saved.as_ref().and_then(|s| s.thinking);
    let remember = resume_settings::settle_thinking(&mut args, remembered);

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
    let persistence = match resumed {
        Some(id) => {
            let typed = typed_this_invocation;
            let kept = resume_settings::resumed_settings(saved, &args, typed, profile, &turn)?;
            let conv = Conversation::resume(ctx.app.chat_history(), id, turn.made_by());
            Some(conv.record_settings(kept).await)
        }
        None => resume_settings::new_conversation(ctx, &args, profile, &turn).await,
    };
    // After the settings a resume replaces whole, which hold what the chat
    // remembered before this session.
    if let (Some(conv), Some(choice)) = (&persistence, remember) {
        conv.remember_thinking(choice).await;
    }

    let params = config::AgentSessionParams {
        model_identifier: args.identifier.clone(),
        profile: selected_profile,
        turn: Some(turn),
        ..config::AgentSessionParams::from(&args)
    };
    // By the rule the daemon resolves a turn's limits with, and only now:
    // `args` holds the flag, or on a resume the limit the chat saved, so the
    // chat's own comes before this machine's stored one, and a new chat has
    // saved only what the command line named.
    let limits = TurnLimits::resolve(args.max_iterations, Some(&settings));
    Ok(Session {
        args,
        params,
        limits,
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
/// The one flag a chat overrides is `--reasoning-budget-tokens`, on a chat
/// with Thinking switched off and no `--thinking` named: `prepare` settles
/// that next ([`resume_settings::settle_thinking`]), and says so on stderr.
/// Which machine it resumes on, and by which id, `prepare` has already taken
/// from the model the conversation stored. The saved settings come back too:
/// whether their profile applies is [`resume_settings::restore_profile`]'s
/// to decide, and what the resume saves in their place is
/// [`resume_settings::resumed_settings`]'s.
async fn resume_conversation(
    ctx: &CliContext,
    args: &ChatArgs,
    conv: gglib_core::domain::chat::Conversation,
) -> Result<(ChatArgs, Vec<AgentMessage>, Option<ConversationSettings>)> {
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
    Ok((merged, prior_messages, conv.settings))
}

#[cfg(test)]
#[path = "running_elsewhere_tests.rs"]
mod running_elsewhere_tests;
