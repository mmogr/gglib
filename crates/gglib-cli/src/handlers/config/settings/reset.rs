//! `gglib config settings reset` — every preference back to its default.
//!
//! What is kept is [`Settings::reset_preferences`]'s to decide. This applies
//! it through [`SettingsRepository::modify`], which the database store runs as
//! one transaction around the read and the write, so the reset keeps the
//! remote fields as they stand when it writes rather than as an earlier read
//! saw them.

use anyhow::Result;
use gglib_core::{CoreError, Settings, SettingsRepository};

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

    reset(ctx.settings_repo.as_ref()).await?;
    println!("✓ Every preference is back to its default. Kept: {KEPT}.");
    Ok(())
}

/// Reset the stored settings in one [`SettingsRepository::modify`].
async fn reset(repo: &dyn SettingsRepository) -> Result<Settings, CoreError> {
    repo.modify(&|settings: &mut Settings| {
        settings.reset_preferences();
        Ok(())
    })
    .await
}

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
