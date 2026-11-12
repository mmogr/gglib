//! `gglib config unset <key>`: return one setting to unset.
//!
//! Unset is a real state, not a synonym for zero or for a default. The one
//! that forced this command is `default-context-size`: leaving it unset is
//! what lets each launch fit the context to the machine, and a stored number
//! outranks that fit. `set` writes a value and never a clear, so without this
//! the only way back was `reset`, which clears every other preference too.

use anyhow::{Result, bail};
use serde_json::Value;

use super::set::not_set_here;
use super::{models_dir, queue_size, wire};
use crate::bootstrap::CliContext;
use crate::config_commands::{SettingKey, Takes};

/// Execute `gglib config unset <key>`.
pub(crate) async fn handle_unset(ctx: &CliContext, key: SettingKey) -> Result<()> {
    match key.takes {
        Takes::Folder => return models_dir::unset(),
        Takes::Elsewhere(_) => bail!(not_set_here(key)),
        _ => {}
    }
    let request = wire::request(&key, Value::Null)?;
    let clears_queue_size = request.max_download_queue_size.is_some();
    ctx.app.settings().update(request.into()).await?;
    println!("✓ {} is now unset.", key.name);
    if clears_queue_size && let Some(warning) = queue_size::tell_daemon(None).await {
        eprintln!("{warning}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "unset_tests.rs"]
mod tests;
