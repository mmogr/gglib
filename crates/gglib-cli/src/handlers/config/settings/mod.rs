#![doc = include_str!("README.md")]
mod models_dir;
mod profiles;
mod queue_size;
mod reset;
mod reveal;
mod set;
mod settings_display;
mod unset;
mod wire;

#[cfg(test)]
#[path = "models_dir_tests.rs"]
mod models_dir_tests;

#[cfg(test)]
#[path = "keys_tests.rs"]
mod keys_tests;

use anyhow::Result;

use crate::bootstrap::CliContext;
use gglib_core::Settings;

use settings_display::{print_sections, settings_display_rows, settings_to_sections};

pub(crate) use profiles::handle_profile;
pub(crate) use reset::handle_reset;
pub(crate) use reveal::handle_reveal;
pub(crate) use set::handle_set;
pub(crate) use unset::handle_unset;

/// The display strings for `default-model-id` and `default-image-model-id`,
/// in that order, each looked up in the library when set.
async fn resolve_model_display(
    ctx: &CliContext,
    settings: &Settings,
) -> Result<(Option<String>, Option<String>)> {
    Ok((
        model_display(ctx, settings.default_model_id).await?,
        model_display(ctx, settings.default_image_model_id).await?,
    ))
}

/// `Some("42 (ModelName)")`, `Some("42 (not found)")`, or `None` for no id.
async fn model_display(ctx: &CliContext, id: Option<i64>) -> Result<Option<String>> {
    match id {
        Some(model_id) => match ctx.app.models().get_by_id(model_id).await? {
            Some(model) => Ok(Some(format!("{} ({})", model_id, model.name))),
            None => Ok(Some(format!("{model_id} (not found)"))),
        },
        None => Ok(None),
    }
}

/// The rows `gglib config show` prints: every setting, and the models
/// directory, which is a line in the data root's `.env` rather than a
/// setting.
async fn shown_rows(ctx: &CliContext) -> Result<Vec<(String, String)>> {
    let settings = ctx.app.settings().get().await?;
    let (model_display, image_display) = resolve_model_display(ctx, &settings).await?;
    let mut rows = settings_display_rows(
        &settings,
        model_display.as_deref(),
        image_display.as_deref(),
    );
    rows.push(models_dir::row(
        &gglib_app_services::models_directory_info()?
    ));
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows)
}

/// Execute `gglib config show`.
pub(crate) async fn handle_show(ctx: &CliContext) -> Result<()> {
    let rows = shown_rows(ctx).await?;
    println!("Current application settings:");
    print_sections(&settings_to_sections(&rows));
    Ok(())
}
