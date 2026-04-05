//! History command handler.
//!
//! Lists past chat conversations with message counts and relative timestamps.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::presentation::{format_relative_time, print_separator, truncate_string};

/// Execute the history command.
///
/// Retrieves and displays past conversations with message counts
/// and relative timestamps for quick browsing.
pub async fn execute(ctx: &CliContext, limit: usize) -> Result<()> {
    let conversations = ctx.app.chat_history().list_conversations().await?;

    if conversations.is_empty() {
        println!("No conversations found.");
        println!("Start one with: gglib chat <model>");
        return Ok(());
    }

    let conversations: Vec<_> = conversations.into_iter().take(limit).collect();

    // Fetch message counts in parallel (repo already has get_message_count)
    let mut rows = Vec::with_capacity(conversations.len());
    for conv in &conversations {
        let count = ctx.app.chat_history().get_message_count(conv.id).await?;
        rows.push((conv, count));
    }

    println!(
        "{:<5} {:<35} {:<6} {:<15} {:<15}",
        "ID", "Title", "Msgs", "Model", "Updated"
    );
    print_separator(80);

    for (conv, msg_count) in &rows {
        let model_label = conv
            .settings
            .as_ref()
            .and_then(|s| s.model_name.as_deref())
            .unwrap_or("--");

        println!(
            "{:<5} {:<35} {:<6} {:<15} {:<15}",
            conv.id,
            truncate_string(&conv.title, 34),
            msg_count,
            truncate_string(model_label, 14),
            format_relative_time(&conv.updated_at),
        );
    }

    println!("\nResume with: gglib chat <model> --continue <ID>");

    Ok(())
}
