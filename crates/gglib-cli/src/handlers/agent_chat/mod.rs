#![doc = include_str!("README.md")]
mod branches;
pub(crate) mod drain;
pub(crate) mod draw;
mod files;
pub(crate) mod images;
mod markdown;
mod memory_jogger;
pub(crate) mod persistence;
mod progress_line;
pub(crate) mod renderer;
pub(crate) mod repl;
mod repl_line;
pub(crate) mod resume_settings;
mod thinking_dispatch;
mod tool_format;
pub(crate) mod turn;

use anyhow::{Result, bail};

use gglib_core::domain::InferenceProfile;
use gglib_core::domain::chat::ConversationSettings;

use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::handlers::inference::chat::ChatArgs;

use self::images::TurnImages;
use self::persistence::Conversation;
use self::turn::Shown;
use crate::target::TurnModel;

/// Entry point: start the interactive agentic REPL.
///
/// The daemon runs the session's turns ([`turn`]): this finds or starts it,
/// makes or resumes the conversation with the settings this command line
/// names, and sends each message typed as a turn on it. When
/// `args.continue_id` is set the conversation resumes as it was saved, with
/// what this command line names in place of what it saved, unless the
/// daemon is replying to it ([`refuse_running_elsewhere`]).
pub(crate) async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    // Before anything is stored: an image, a setting, a row.
    if let Some(id) = args.continue_id {
        refuse_running_elsewhere(id).await?;
    }
    // A file that cannot be attached ends the command before a conversation
    // is made or a model asked for.
    let (attachments, mut receipts) = (ctx.app.attachments(), std::io::stderr());
    let images = TurnImages::attach(attachments, &args.images, false, &mut receipts).await?;
    let (args, plan) = plan(ctx, args).await?;
    // The daemon runs the turns: found, or started, once the model is known
    // and before anything is written, so a chat is never made for a turn
    // nothing can answer.
    let daemon = daemon_client::ensure_daemon().await?;
    let kept = plan
        .resumed
        .as_ref()
        .map(|resumed| resumed.held.folder.clone());
    let conversation = save(ctx, &args, plan).await?;

    // The session's Draw switch (`/draw`), and the folder its file tools read.
    let draw = draw::DrawSwitch::for_session(&daemon, args.target);
    let sent_to = conversation.machine.as_deref();
    let chat = kept.as_ref().map_or(files::Chat::New, |folder| {
        files::Chat::Resumed(folder.as_deref())
    });
    let (folder, files_line) = files::of_session(&args, sent_to, chat);
    if let Some(line) = files_line {
        eprintln!("{line}");
    }

    let session = repl::Session {
        daemon,
        conversation,
        images,
        draw,
        folder,
        thinking: args.thinking,
        shown: Shown {
            verbose: args.verbose,
            quiet: false,
        },
    };
    repl::run_repl(session).await
}

/// Refuse to continue chat `id` while the daemon is replying to it for the
/// page or a paired device, and so writing its rows: the rule the daemon
/// refuses a second run by ([`gglib_core::domain::runs::RunInfo::holds`]),
/// read off its own listing. Refused only on the daemon's word. With none
/// running no daemon run is replying, and none is started to ask; one that
/// does not list its runs for this data root's token, as another data
/// root's does not, has said nothing of this chat.
async fn refuse_running_elsewhere(id: i64) -> Result<()> {
    if let Ok(daemon) = daemon_client::running().await
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

/// What a session writes before its first turn, decided with nothing
/// written yet.
struct Plan {
    /// The model, as the machine that serves the turn resolved it.
    turn: TurnModel,
    /// The profile selected here, if any.
    profile: Option<InferenceProfile>,
    /// The chat resumed, when one is.
    resumed: Option<Resumed>,
}

/// A chat a session resumes.
struct Resumed {
    id: i64,
    /// The settings the chat saved, as this build reads them, with the
    /// folder the chat remembers, which the session keeps.
    held: ConversationSettings,
    /// The settings it saves in place of `held`; `None` changes none.
    kept: Option<ConversationSettings>,
}

/// [`plan`], then [`save`]: what [`run`] does before its first turn, but
/// for the daemon it finds or starts between them.
#[cfg(test)]
async fn prepare<'a>(ctx: &'a CliContext, args: &ChatArgs) -> Result<(ChatArgs, Conversation<'a>)> {
    let (args, plan) = plan(ctx, args).await?;
    let conversation = save(ctx, &args, plan).await?;
    Ok((args, conversation))
}

/// Make or resume the conversation as `plan` says, with what this command
/// line names. The conversation is the session's: the daemon reads its
/// settings on every turn, and saves the turn to it.
async fn save<'a>(ctx: &'a CliContext, args: &ChatArgs, plan: Plan) -> Result<Conversation<'a>> {
    let Plan {
        turn,
        profile,
        resumed,
    } = plan;
    let history = ctx.app.chat_history();
    let Some(Resumed { id, held, kept }) = resumed else {
        let settings = resume_settings::session_settings(args, profile.as_ref(), &turn);
        let prompt = args.system_prompt.clone();
        return Conversation::create(history, prompt, settings, turn.machine).await;
    };
    let conv = Conversation::resume(history, id, turn.machine);
    conv.record_settings(&held, kept).await?;
    if let Some(prompt) = &args.system_prompt {
        conv.record_system_prompt(prompt).await?;
    }
    Ok(conv)
}

/// Read the settings, settle the model and the profile, and decide what the
/// session saves, writing nothing: a model this machine does not have is
/// refused here.
async fn plan(ctx: &CliContext, args: &ChatArgs) -> Result<(ChatArgs, Plan)> {
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
    let mut selected_profile = if let Some(selection) =
        crate::handlers::inference::profile_selection::select_before_resume(
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
        selection.profile
    } else {
        None
    };

    let resumed = if let Some(conv) = stored {
        let id = conv.id;
        Some((id, resume_conversation(ctx, &mut args, conv).await?))
    } else {
        args.identifier = args
            .target
            .model_for_turn(ctx, std::mem::take(&mut args.identifier), async || {
                bail!("model identifier is required (use --continue <ID> to resume a session)")
            })
            .await?;
        None
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

    // Resolved once, on the machine that serves the turn, before anything
    // is saved: a model that is not there is refused here. A resume on
    // another model than the conversation stored then stores that one.
    let turn = args
        .target
        .resolve_turn(ctx, std::mem::take(&mut args.identifier))
        .await?;
    args.identifier.clone_from(&turn.identifier);
    let profile = selected_profile.as_ref();
    let resumed = if let Some((id, saved)) = resumed {
        let typed = typed_this_invocation;
        let held = saved.unwrap_or_default();
        let kept =
            resume_settings::resumed_settings(ctx, &held, &args, typed, profile, &turn).await?;
        Some(Resumed { id, held, kept })
    } else {
        resume_settings::require_a_model(ctx, &turn).await?;
        None
    };
    let plan = Plan {
        turn,
        profile: selected_profile,
        resumed,
    };
    Ok((args, plan))
}

/// Show a previous conversation's last turns, and take from what it saved
/// the model `args` resumes with when the command line names none and the
/// conversation stored none by its machine. Its settings come back whole:
/// what the resume saves in their place is
/// [`resume_settings::resumed_settings`]'s to say. Which machine it resumes
/// on, and by which id, `prepare` has already taken from the model the
/// conversation stored.
async fn resume_conversation(
    ctx: &CliContext,
    args: &mut ChatArgs,
    conv: gglib_core::domain::chat::Conversation,
) -> Result<Option<ConversationSettings>> {
    let history = ctx.app.chat_history();
    let conv_id = conv.id;

    let db_messages = history.get_messages(conv_id).await?;
    if db_messages.is_empty() {
        println!("Conversation #{conv_id} has no messages — starting fresh.");
    } else {
        print!(
            "{}",
            memory_jogger::memory_jogger(&db_messages, &conv.title)
        );
    }

    if args.identifier.is_empty()
        && let Some(name) = conv.settings.as_ref().and_then(|s| s.model_name.as_ref())
    {
        args.identifier.clone_from(name);
    }
    if args.identifier.is_empty() {
        bail!(
            "cannot resume conversation #{conv_id}: no model name was saved and none was provided on the CLI"
        );
    }
    Ok(conv.settings)
}

#[cfg(test)]
#[path = "running_elsewhere_tests.rs"]
mod running_elsewhere_tests;
