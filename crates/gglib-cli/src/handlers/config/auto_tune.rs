//! `gglib config auto-tune` — status and the setup-time consent prompt.
//!
//! The idle-time auto-tune scheduler is opt-in by project decision:
//! autonomy that spends the GPU is a thing an operator turns on. This
//! command is how the question gets *asked* — `make setup` runs the prompt
//! beside the models-directory and fast-downloads prompts, so every CLI
//! install sees the offer once, with yes as the recommended answer, instead
//! of the feature shipping dark.

use std::io::IsTerminal as _;
use std::io::Write as _;

use anyhow::Result;
use gglib_core::SettingsUpdate;

use crate::bootstrap::CliContext;
use crate::config_commands::AutoTuneCommand;
use crate::presentation::style::{INFO, RESET, SUCCESS};

/// Route an `auto-tune` subcommand.
pub async fn dispatch(ctx: &CliContext, command: Option<AutoTuneCommand>) -> Result<()> {
    match command.unwrap_or(AutoTuneCommand::Status) {
        AutoTuneCommand::Status => status(ctx).await,
        AutoTuneCommand::Prompt => prompt(ctx).await,
    }
}

async fn status(ctx: &CliContext) -> Result<()> {
    let enabled = ctx.app.settings().get().await?.auto_tune == Some(true);
    if enabled {
        println!("{SUCCESS}✓ Idle-time auto-tune is enabled{RESET}");
    } else {
        println!(
            "{INFO}Idle-time auto-tune is off. Enable with:{RESET} \
             gglib config settings set --auto-tune true"
        );
    }
    Ok(())
}

/// Offer to enable auto-tune, interactively. Skips without a terminal.
async fn prompt(ctx: &CliContext) -> Result<()> {
    if ctx.app.settings().get().await?.auto_tune == Some(true) {
        println!("{SUCCESS}✓ Idle-time auto-tune is already enabled{RESET}");
        return Ok(());
    }

    if !std::io::stdin().is_terminal() {
        // No terminal is not an answer. Say what did not happen and how to
        // do it later, rather than assuming either way on the user's behalf.
        println!(
            "{INFO}Idle-time auto-tune stays off. To let gglib tune models \
             during idle GPU time:{RESET} gglib config settings set --auto-tune true"
        );
        return Ok(());
    }

    println!("Let gglib tune your models during idle GPU time? (recommended)");
    println!("  When the GPU has been idle a while, gglib measures untuned models and");
    println!("  applies better sampling defaults — only when the evidence clears a");
    println!("  statistical gate. Warm models are never evicted, your own settings are");
    println!("  never touched, and any request preempts the run instantly.");
    print!("Enable idle-time auto-tune? [Y/n] ");
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).ok();
    let declined = matches!(answer.trim().to_lowercase().as_str(), "n" | "no");

    if declined {
        println!(
            "{INFO}Leaving it off. Enable later with:{RESET} \
             gglib config settings set --auto-tune true"
        );
        return Ok(());
    }

    ctx.app
        .settings()
        .update(SettingsUpdate {
            auto_tune: Some(Some(true)),
            ..Default::default()
        })
        .await?;
    println!("{SUCCESS}✓ Idle-time auto-tune enabled{RESET}");
    Ok(())
}
