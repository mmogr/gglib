//! List command handler.
//!
//! Fetches and displays GGUF models with optional sort / filter flags. The
//! rows are [`ModelOps::list_with_query`]'s, the listing `GET /api/models`
//! answers with, so a flag here and a filter on the library page narrow by
//! one rule; a single table function renders them. A speed column (`⚡ t/s`)
//! is shown only when at least one returned model has a latest generation
//! speed. A `Size` column says what each model takes on disk (weights,
//! projector and components), an `Images` column says which models are
//! linked to a projector, and the `Draws` column names an image model's family, and the components it still needs
//! linked before it can draw. While this machine is paired, the list ends
//! with one line on how the paired machine stands, which asks that machine
//! nothing.

use std::fmt::Write as _;

use anyhow::Result;
use gglib_app_services::ModelOps;
use gglib_app_services::types::GuiModel;
use gglib_core::domain::ModelListQuery;
use gglib_core::download::format_size;

use crate::bootstrap::CliContext;
use crate::model_list_args::ListArgs;
use crate::presentation::{format_params, truncate_string};
use crate::target::Target;

#[path = "list_far.rs"]
mod list_far;

// ─────────────────────────────────────────────────────────────────────────────
// Public surface
// ─────────────────────────────────────────────────────────────────────────────

/// Execute the list command: this machine's catalogue, or the paired
/// machine's as its proxy publishes it.
pub(crate) async fn execute(
    target: Target,
    ctx: &CliContext,
    ops: &ModelOps,
    args: ListArgs,
) -> Result<()> {
    target
        .run(
            async || list_here(ctx, ops, args).await,
            async || list_far::execute(target).await,
        )
        .await
}

/// This machine's catalogue, sorted and filtered as asked, and the paired
/// machine's line when there is one.
async fn list_here(ctx: &CliContext, ops: &ModelOps, args: ListArgs) -> Result<()> {
    let models = fetch_models(ops, &args).await?;
    let paired = list_far::summary(ctx).await;
    print!("{}", listing(&models, paired.as_deref()));
    Ok(())
}

/// What `gglib model list` prints: the table, or the line that stands in
/// for an empty one, and then `paired`. An empty library still gets
/// `paired`: a laptop with no models of its own, paired with a desktop that
/// has them, is the case the line is for.
fn listing(models: &[GuiModel], paired: Option<&str>) -> String {
    let mut out = if models.is_empty() {
        "No models found.\nUse 'gglib model add <file_path>' to add your first model.\n".to_owned()
    } else {
        format!(
            "Found {} model(s):\n\n{}",
            models.len(),
            render_table(models)
        )
    };
    if let Some(line) = paired {
        let _ = write!(out, "\n{line}\n");
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Fetch helpers
// ─────────────────────────────────────────────────────────────────────────────

async fn fetch_models(ops: &ModelOps, args: &ListArgs) -> Result<Vec<GuiModel>> {
    Ok(ops.list_with_query(build_query(args)).await?)
}

fn build_query(args: &ListArgs) -> ModelListQuery {
    ModelListQuery {
        sort_by: args.sort.into(),
        order: args.order.into(),
        min_params: args.min_params,
        max_params: args.max_params,
        min_speed: args.min_speed,
        max_speed: args.max_speed,
        tags: if args.tags.is_empty() {
            None
        } else {
            Some(args.tags.clone())
        },
        ..Default::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Table rendering
// ─────────────────────────────────────────────────────────────────────────────

/// What the `Draws` column says of `model`: `--` for a model that chats,
/// its family's name for an image model with every component linked, and
/// the roles it still needs after the name for one without.
fn draws(model: &GuiModel) -> String {
    let Some(family) = model.image_family else {
        return "--".to_owned();
    };
    if model.missing_components.is_empty() {
        return family.label().to_owned();
    }
    let missing: Vec<&str> = model
        .missing_components
        .iter()
        .map(|role| role.label())
        .collect();
    format!("{}, needs {}", family.label(), missing.join(", "))
}

/// The generation speed a row's speed cell prints: the latest measured.
fn latest_speed(model: &GuiModel) -> Option<f64> {
    model.benchmark_summary.as_ref()?.latest_tg_tps
}

/// The table, as text. The ID column is as wide as the widest id, so a
/// four-digit id does not push its row out of line with the rest, and the
/// `Draws` column is as wide as its widest entry, so the roles an image
/// model needs are never cut.
#[expect(
    clippy::too_many_lines,
    reason = "one table in two layouts, with and without the speed column; the rows read whole"
)]
fn render_table(models: &[GuiModel]) -> String {
    // The column shows when a row has a speed to print, not merely a summary:
    // a library benchmarked only by compare runs has summaries without one.
    let show_speed = models.iter().any(|m| latest_speed(m).is_some());
    let id_width = models
        .iter()
        .map(|m| m.id.to_string().len())
        .max()
        .unwrap_or(0)
        .max(3);
    // The id a client sends, which is the name unless twins share it.
    let model_ids: Vec<&str> = models
        .iter()
        .map(|m| m.openai_id.as_deref().unwrap_or("--"))
        .collect();
    let model_id_width = model_ids
        .iter()
        .map(|id| id.chars().count())
        .max()
        .unwrap_or(0)
        .max("Model ID".len());
    let drawn: Vec<String> = models.iter().map(draws).collect();
    let draws_width = drawn
        .iter()
        .map(|cell| cell.chars().count())
        .max()
        .unwrap_or(0)
        .max("Draws".len());
    let mut out = String::new();

    if show_speed {
        let _ = writeln!(
            out,
            "{:<id_width$} {:<25} {:<model_id_width$} {:<8} {:<10} {:<12} {:<8} {:<10} {:<10} {:<7} {:<draws_width$} {:<20} File Path",
            "ID",
            "Name",
            "Model ID",
            "Params",
            "⚡ t/s",
            "Arch",
            "Quant",
            "Size",
            "Context",
            "Images",
            "Draws",
            "Added"
        );
        let _ = writeln!(
            out,
            "{}",
            "-".repeat(146 + id_width + model_id_width + draws_width)
        );
    } else {
        let _ = writeln!(
            out,
            "{:<id_width$} {:<25} {:<model_id_width$} {:<8} {:<12} {:<8} {:<10} {:<10} {:<7} {:<draws_width$} {:<20} File Path",
            "ID",
            "Name",
            "Model ID",
            "Params",
            "Arch",
            "Quant",
            "Size",
            "Context",
            "Images",
            "Draws",
            "Added"
        );
        let _ = writeln!(
            out,
            "{}",
            "-".repeat(133 + id_width + model_id_width + draws_width)
        );
    }

    for ((model, draws), model_id) in models.iter().zip(&drawn).zip(&model_ids) {
        let arch = model.architecture.as_deref().unwrap_or("--");
        let quant = model.quantization.as_deref().unwrap_or("--");
        let context = model
            .context_length
            .map_or_else(|| "--".to_string(), |c| c.to_string());
        // A model reads images exactly when it is linked to a projector.
        let images = if model.image_input { "yes" } else { "--" };
        // What it takes on disk, all its files; `--` when none could be read.
        let size = if model.size_bytes == 0 {
            "--".to_owned()
        } else {
            format_size(model.size_bytes)
        };

        if show_speed {
            let speed = latest_speed(model).map_or_else(|| "--".to_string(), |t| format!("{t:.1}"));
            let _ = writeln!(
                out,
                "{:<id_width$} {:<25} {:<model_id_width$} {:<8} {:<10} {:<12} {:<8} {:<10} {:<10} {:<7} {:<draws_width$} {:<20} {}",
                model.id,
                truncate_string(&model.name, 24),
                model_id,
                format_params(model.param_count_b),
                truncate_string(&speed, 9),
                truncate_string(arch, 11),
                truncate_string(quant, 7),
                size,
                truncate_string(&context, 9),
                images,
                draws,
                model.added_at,
                model.file_path,
            );
        } else {
            let _ = writeln!(
                out,
                "{:<id_width$} {:<25} {:<model_id_width$} {:<8} {:<12} {:<8} {:<10} {:<10} {:<7} {:<draws_width$} {:<20} {}",
                model.id,
                truncate_string(&model.name, 24),
                model_id,
                format_params(model.param_count_b),
                truncate_string(arch, 11),
                truncate_string(quant, 7),
                size,
                truncate_string(&context, 9),
                images,
                draws,
                model.added_at,
                model.file_path,
            );
        }
    }
    out
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
