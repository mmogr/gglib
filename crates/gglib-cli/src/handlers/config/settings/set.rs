//! `gglib config set <key> <value>`: write one setting, and print it.
//!
//! The value is read by what the key takes, in [`SettingKey`]'s table, and
//! put on the wire type under the key's own name. The merge, the validation
//! and the write are the settings service's, in one step, so a refused value
//! stores nothing and its refusal is the validator's own sentence.

use anyhow::{Result, anyhow};
use serde_json::Value;

use gglib_core::Settings;

use super::settings_display::settings_display_rows;
use super::{models_dir, queue_size, resolve_model_display, wire};
use crate::bootstrap::CliContext;
use crate::config_commands::{SettingKey, Takes};
use crate::handlers::model::resolver;

/// Execute `gglib config set <key> <value>`.
pub(crate) async fn handle_set(ctx: &CliContext, key: SettingKey, typed: &str) -> Result<()> {
    let value = match key.takes {
        Takes::Folder => return models_dir::set(typed),
        // Resolved here, as every command naming a model resolves it;
        // whether an image model draws is the settings service's check.
        Takes::Model => Value::from(resolver::resolve_model_identifier(ctx, typed).await?.id),
        takes => read(takes, typed).ok_or_else(|| refusal(key, typed))?,
    };
    let request = wire::request(&key, value).map_err(|_| refusal(key, typed))?;

    let sets_queue_size = request.max_download_queue_size.is_some();
    let updated = ctx.app.settings().update(request.into()).await?;
    let shown = shown(ctx, &updated, key.name).await?;
    println!(
        "✓ {} is now {}",
        key.name,
        shown.as_deref().unwrap_or(typed)
    );
    if sets_queue_size
        && let Some(warning) = queue_size::tell_daemon(updated.max_download_queue_size).await
    {
        eprintln!("{warning}");
    }
    Ok(())
}

/// `typed` as the JSON value `takes` reads it as, or `None` when it is not
/// one. A model and a folder are read by their handlers, not here.
fn read(takes: Takes, typed: &str) -> Option<Value> {
    match takes {
        Takes::Number(_) => typed.trim().parse::<u64>().ok().map(Value::from),
        // What clap's own `bool` parser takes, and nothing else.
        Takes::Switch => match typed {
            "true" => Some(true.into()),
            "false" => Some(false.into()),
            _ => None,
        },
        Takes::Text => Some(typed.into()),
        // As the app's comma field reads it.
        Takes::Hosts => Some(
            typed
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .map(Value::from)
                .collect(),
        ),
        Takes::Model | Takes::Folder | Takes::Whole(_) | Takes::Elsewhere(_) => None,
    }
}

/// The refusal of a value the key does not take, saying what it does.
fn refusal(key: SettingKey, typed: &str) -> anyhow::Error {
    anyhow!(
        "`{}` takes {}, got '{typed}'",
        key.name,
        key.takes.described()
    )
}

/// The value `config show` prints for `name` in `settings`: a secret as
/// held rather than as typed, a model by its name.
async fn shown(ctx: &CliContext, settings: &Settings, name: &str) -> Result<Option<String>> {
    let (model, image) = resolve_model_display(ctx, settings).await?;
    Ok(
        settings_display_rows(settings, model.as_deref(), image.as_deref())
            .into_iter()
            .find_map(|(key, value)| (key == name).then_some(value)),
    )
}

#[cfg(test)]
#[path = "set_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "set_drawing_tests.rs"]
mod drawing_tests;
