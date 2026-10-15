//! `gglib config settings reset` — every preference back to its default.
//!
//! What is kept is [`gglib_core::Settings::reset_preferences`]'s to decide,
//! and the write is the settings service's, like `set` and `unset`: the
//! database store runs it as one transaction around the read and the write,
//! so the reset keeps the remote fields as they stand when it writes rather
//! than as an earlier read saw them. A row this build cannot read does not
//! stop it: a preference's is dropped, a kept one is left, and both are named.

use anyhow::Result;
use gglib_core::ports::SettingsReset;

use crate::bootstrap::CliContext;
use crate::utils::input::prompt_confirmation;

/// What a reset leaves alone, as the prompt and the result say it.
const KEPT: &str = "the machine this one joined, the devices it admits, \
                    whether and how it serves remote access, and the proxy's API key";

pub(super) async fn handle_reset(ctx: &CliContext, force: bool) -> Result<()> {
    if !force
        && !prompt_confirmation(&format!(
            "Reset every preference to its default? Kept: {KEPT}."
        ))?
    {
        println!("Reset cancelled.");
        return Ok(());
    }

    let reset = ctx.app.settings().reset_preferences().await?;
    println!("✓ Every preference is back to its default. Kept: {KEPT}.");
    for line in unreadable_lines(&reset) {
        eprintln!("{line}");
    }
    Ok(())
}

/// What the reset says of the rows it could not read, one line each kind.
fn unreadable_lines(reset: &SettingsReset) -> Vec<String> {
    let mut lines = Vec::new();
    if !reset.dropped.is_empty() {
        lines.push(format!(
            "Dropped, because this build cannot read them: {}.",
            reset.dropped.join(", ")
        ));
    }
    if !reset.kept_unreadable.is_empty() {
        lines.push(format!(
            "⚠ Kept, although this build cannot read them: {}. A reset never \
             drops these; reading the settings fails until they are fixed.",
            reset.kept_unreadable.join(", ")
        ));
    }
    lines
}

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
