//! Single-turn agentic question handler for `gglib q`.
//!
//! Makes a conversation whose file tools read the directory it was asked
//! in, by `gglib chat`'s rule (`agent_chat::files`: never home or a system
//! folder), sends the question as one turn through the daemon's door, shows
//! the reply as it arrives, and optionally goes on as an interactive chat on
//! the same conversation if the user wants to continue it.

use std::io;

use anyhow::{Result, anyhow};

use gglib_core::domain::hub_chats::LocalTurn;

use crate::bootstrap::CliContext;
use crate::conversation_settings::ConversationSettingsBuilder;
use crate::daemon_client;
use crate::handlers::agent_chat::files::{self, Chat};
use crate::handlers::agent_chat::images::TurnImages;
use crate::handlers::agent_chat::persistence::Conversation;
use crate::handlers::agent_chat::repl::{Session, run_repl};
use crate::handlers::agent_chat::resume_settings::require_a_model;
use crate::handlers::agent_chat::turn::Shown;
use crate::shared_args::ChatSamplingArgs;
use crate::target::Target;
use crate::utils::input::Input;

/// System prompt for a question asked with its tools.
const SYSTEM_PROMPT: &str = "\
You are an expert code analyst. You have access to filesystem tools \
(read_file, list_directory, grep_search) scoped to the user's working \
directory. Use them to explore the codebase and answer the question \
thoroughly. Be direct and concise.";

/// System prompt for a question asked with `--no-tools`. It names no tool
/// and no folder: a model told of tools it was not given calls one anyway,
/// and the call comes back as its reply. It does not say there are no
/// tools either: the prompt is saved with the chat, and a later message
/// sent with `/draw` is given the image tool, which a model told it has
/// none argues with.
const SYSTEM_PROMPT_NO_TOOLS: &str = "\
Answer the question from what you know and from any text given with it. \
You cannot read files. Be direct and concise.";

/// System prompt for a question asked with its tools in a directory whose
/// files are not read: home, a system folder, or under a `--tools` list
/// that admits no file tool. It names no file tool and no folder, for the
/// reason [`SYSTEM_PROMPT_NO_TOOLS`] names none.
const SYSTEM_PROMPT_NO_FOLDER: &str = "\
Answer the question from what you know, from any text given with it and \
with any tool you are given. You cannot read files here. Be direct and \
concise.";

/// The system prompt a question's chat is made with. Only a question whose
/// file tools read a folder is told of them and of that folder.
fn system_prompt(no_tools: bool, folder: Option<&str>) -> String {
    match (no_tools, folder) {
        (true, _) => SYSTEM_PROMPT_NO_TOOLS.to_owned(),
        (false, Some(folder)) => format!("{SYSTEM_PROMPT}\n\nWorking directory: {folder}"),
        (false, None) => SYSTEM_PROMPT_NO_FOLDER.to_owned(),
    }
}

/// Arguments for the `q` command.
///
/// A bag rather than a parameter list, matching how `chat` already passes
/// [`ChatArgs`](super::chat::ChatArgs): fifteen positional arguments made
/// every call site a counting exercise and needed
/// `#[allow(clippy::too_many_arguments)]` to compile clean.
#[expect(
    clippy::struct_excessive_bools,
    reason = "four independent flags as typed, none a state of another"
)]
pub(crate) struct QuestionArgs {
    pub question: String,
    pub model_arg: Option<String>,
    pub file: Option<String>,
    /// Which machine the turn runs on (ADR 0013).
    pub target: Target,
    pub max_iterations: Option<usize>,
    pub tools: Vec<String>,
    /// `--no-tools`: the question's chat calls no tool.
    pub no_tools: bool,
    pub tool_timeout_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    /// `--image`: the files attached to the question.
    pub images: Vec<std::path::PathBuf>,
    /// `--show-prompt`: echo the assembled user message before sending.
    pub show_prompt: bool,
    pub verbose: bool,
    pub quiet: bool,
    pub sampling: ChatSamplingArgs,
    /// Named sampling profile, the flag form of a `{model}:{profile}` suffix.
    pub profile: Option<String>,
}

/// Run a single-turn agentic question, with optional continuation into chat.
#[expect(
    clippy::too_many_lines,
    reason = "one `gglib q` read top to bottom: input, model and profile, the conversation, \
              the turn, and the hand-off to chat"
)]
pub(crate) async fn execute(ctx: &CliContext, args: QuestionArgs) -> Result<()> {
    let QuestionArgs {
        question,
        model_arg,
        file,
        target,
        max_iterations,
        tools,
        no_tools,
        tool_timeout_ms,
        max_parallel,
        images,
        show_prompt,
        verbose,
        quiet,
        sampling,
        profile,
    } = args;
    // A file that cannot be attached ends the command before any model lookup.
    let (attachments, mut receipts) = (ctx.app.attachments(), io::stderr());
    let mut images = TurnImages::attach(attachments, &images, quiet, &mut receipts).await?;

    // If no model was specified, look up the default from settings
    let settings = ctx
        .app
        .settings()
        .get()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?;

    // Resolve `--profile` or a `{model}:{profile}` suffix before the model is
    // looked up — the suffix must not reach lookup. What that means on the
    // paired machine, whose profiles these are not, is `profile_selection`'s
    // to say.
    let selection = super::profile_selection::select_for_upstream(
        target,
        ctx.catalog.as_ref(),
        settings.inference_profiles.as_deref().unwrap_or_default(),
        model_arg.as_deref().unwrap_or_default(),
        profile.as_deref(),
    )
    .await?;
    // A turn with no model named: this machine's default here, by id, the
    // model last asked for there — then resolved once, on that machine.
    let default_id = settings.default_model_id;
    let model_identifier = target
        .model_for_turn(ctx, selection.model, async || {
            let default_id = default_id.ok_or_else(|| {
                anyhow!(
                    "No model specified and no default model set.\n\
                     Use --model <id-or-name> or set a default:\n  \
                     gglib config set default-model-id <id-or-name>"
                )
            })?;
            let model = ctx
                .app
                .models()
                .get_by_id(default_id)
                .await
                .map_err(|e| anyhow!("failed to load default model: {e}"))?
                .ok_or_else(|| anyhow!("default model (ID: {default_id}) not found"))?;
            Ok(model.id.to_string())
        })
        .await?;
    let turn = target.resolve_turn(ctx, model_identifier).await?;
    require_a_model(ctx, &turn).await?;
    // The daemon answers the question: found, or started, once its model is.
    let daemon = daemon_client::ensure_daemon().await?;

    // The file tools read the directory the question was asked in, by
    // `gglib chat`'s rule. Only a question that reads none says so, so a
    // question that reads its folder prints what it always has.
    let sent_to = turn.machine.as_deref();
    let (folder, files_line) = files::of_session(&tools, no_tools, sent_to, Chat::New);
    if let (None, Some(line), false) = (&folder, &files_line, quiet) {
        eprintln!("{line}");
    }

    // The question's chat, made before it is asked: the daemon answers a
    // turn on a saved conversation, and saves the turn to it.
    let system_prompt = system_prompt(no_tools, folder.as_deref());
    let profile_name = selection
        .profile
        .as_ref()
        .map(|p| p.name.clone())
        .or_else(|| turn.far_profile.clone());
    let chat_settings = ConversationSettingsBuilder::new(&sampling)
        .turn(&turn)
        .profile(profile_name)
        .tools(tools, no_tools)
        .agent_params(max_iterations, tool_timeout_ms, max_parallel)
        .build();
    let (chats, machine) = (ctx.app.chat_history(), turn.machine.clone());
    let conversation =
        Conversation::create(chats, Some(system_prompt), chat_settings, machine).await?;

    // Construct user message with optional piped/file context
    let user_content =
        super::question_input::build_user_message(&question, file.as_deref(), show_prompt)?;
    let shown = Shown { verbose, quiet };
    let asked = LocalTurn {
        conversation_id: conversation.id,
        content: user_content,
        images: images.take(),
        thinking: None,
        answer_saved: false,
        draw: false,
        image_model: None,
        folder: folder.clone(),
    };
    // The session `gglib chat` would make, with no Thinking choice: `q`
    // names none. Its first turn, the question, never draws; a line typed
    // after `y` may, with `/draw`.
    let session = Session::new(daemon, conversation, images, target, folder, None, shown);
    let completed = session.send(&asked).await;

    // ── Continuation prompt ──────────────────────────────────────────────
    // Offer to continue chatting if the initial question succeeded and we
    // are in an interactive terminal.  Skip when:
    //   - quiet mode (--quiet) — script-friendly output
    //   - stdin is not a TTY (piped input) — would read garbage or hang
    let interactive = !quiet && Input::stdin() == Input::Terminal;

    if !completed {
        return Err(anyhow!("agent did not produce a final answer"));
    }
    if interactive && super::question_input::ask_continue()? {
        run_repl(session).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "agent_question_tests.rs"]
mod agent_question_tests;
