//! History command handler.
//!
//! Lists past chat conversations with message counts, the chat each branch
//! was made from, and relative timestamps.

use anyhow::Result;
use gglib_app_services::far_credentials;
use gglib_core::RemotePairing;
use gglib_core::domain::chat::ConversationSettings;
use gglib_core::domain::{Machine, UNNAMED_PAIRED};

use crate::bootstrap::CliContext;
use crate::presentation::{format_relative_time, print_separator, truncate_string};

/// Execute the history command.
///
/// Retrieves and displays past conversations with message counts
/// and relative timestamps for quick browsing.
pub(crate) async fn execute(ctx: &CliContext, limit: usize) -> Result<()> {
    let conversations = ctx.app.chat_history().list_conversations().await?;

    if conversations.is_empty() {
        println!("No conversations found.");
        println!("Start one with: gglib chat <model>");
        return Ok(());
    }

    let conversations: Vec<_> = conversations.into_iter().take(limit).collect();
    let pairing = ctx.app.settings().get().await?.remote_pairing;

    // Fetch message counts in parallel (repo already has get_message_count)
    let mut rows = Vec::with_capacity(conversations.len());
    for conv in &conversations {
        let count = ctx.app.chat_history().get_message_count(conv.id).await?;
        rows.push((conv, count));
    }

    println!(
        "{:<5} {:<35} {:<6} {:<9} {:<40} {:<15}",
        "ID", "Title", "Msgs", "Branched", "Model", "Updated"
    );
    print_separator(115);

    for (conv, msg_count) in &rows {
        let model_label = conv.settings.as_ref().map_or_else(
            || "--".to_owned(),
            |s| model_label(s, pairing.as_ref(), MODEL_WIDTH),
        );

        println!(
            "{:<5} {:<35} {:<6} {:<9} {:<40} {:<15}",
            conv.id,
            truncate_string(&conv.title, 34),
            msg_count,
            branched(conv.branch_of),
            model_label,
            format_relative_time(&conv.updated_at),
        );
    }

    println!("\nResume with: gglib chat --continue <ID>");

    Ok(())
}

/// The Branched cell: the chat a branch was made from (ADR 0017), and
/// nothing for a chat that is no branch.
fn branched(branch_of: Option<i64>) -> String {
    branch_of.map_or_else(String::new, |id| format!("from #{id}"))
}

/// The most a Model cell shows, one short of its column.
const MODEL_WIDTH: usize = 39;

/// The model a conversation ran on, as a person reads it, in at most `width`
/// characters: `qwen3 (3) on desk`, `qwen3 (3) on this machine`, or the bare
/// name a row that stores no model was saved with. A long name is shortened,
/// never the `(id) on machine` after it, which says where the chat resumes.
/// A machine named by a pairing this one no longer holds is shown as
/// another machine; its fingerprint is never shown.
fn model_label(
    settings: &ConversationSettings,
    pairing: Option<&RemotePairing>,
    width: usize,
) -> String {
    let name = settings.model_name.as_deref().unwrap_or("--");
    let Some(model) = &settings.model else {
        return truncate_string(name, width);
    };
    let machine = match &model.machine {
        Machine::Local => "this machine",
        Machine::Paired { fingerprint } => match pairing {
            Some(stored) if far_credentials(Some(stored), fingerprint).is_ok() => {
                stored.name.as_deref().unwrap_or(UNNAMED_PAIRED)
            }
            _ => "another machine",
        },
    };
    let place = format!(" ({}) on {machine}", model.id);
    let room = width.saturating_sub(place.chars().count()).max(1);
    format!("{}{place}", truncate_string(name, room))
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
