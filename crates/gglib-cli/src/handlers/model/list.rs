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
//! nothing. In a terminal the table fits its width
//! ([`crate::presentation::table_layout`]); piped, it is printed whole.

use std::fmt::Write as _;

use anyhow::Result;
use gglib_app_services::ModelOps;
use gglib_app_services::types::GuiModel;
use gglib_core::domain::ModelListQuery;
use gglib_core::download::format_size;

use crate::bootstrap::CliContext;
use crate::model_list_args::ListArgs;
use crate::presentation::table_layout::{Column, Shorten, Table};
use crate::presentation::terminal::stdout_width;
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
    print!("{}", listing(&models, paired.as_deref(), stdout_width()));
    Ok(())
}

/// What `gglib model list` prints: the table, or the line that stands in
/// for an empty one, and then `paired`. An empty library still gets
/// `paired`: a laptop with no models of its own, paired with a desktop that
/// has them, is the case the line is for. The table fits `width`, the
/// terminal's, or is whole when there is none.
fn listing(models: &[GuiModel], paired: Option<&str>, width: Option<usize>) -> String {
    let mut out = if models.is_empty() {
        "No models found.\nUse 'gglib model add <file_path>' to add your first model.\n".to_owned()
    } else {
        format!(
            "Found {} model(s):\n\n{}",
            models.len(),
            render_table(models, width)
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

/// The columns, in the order they are printed. When the terminal is too
/// narrow the lowest priority goes first: what a model is called and the
/// id every command takes are never dropped; then which file it is (its
/// quantization and size), where that file is (the path, shortened to its
/// file name before it goes), its parameters and context, the id a client
/// sends (the name unless twins share it), what it draws and whether it
/// reads images, its architecture, its speed, and last, when it was added.
fn columns(show_speed: bool) -> Vec<Column> {
    let mut columns = vec![
        // A four-digit id does not push its row out of line with the rest.
        Column::new("ID").pad(3),
        Column::new("Name")
            .pad(25)
            .shorten(Shorten::End { min: 16 }),
        Column::new("Model ID")
            .droppable(5)
            .shorten(Shorten::End { min: 12 }),
        Column::new("Params").pad(8).droppable(7),
    ];
    if show_speed {
        columns.push(Column::new("⚡ t/s").pad(10).droppable(1));
    }
    columns.extend([
        Column::new("Arch").pad(12).droppable(2),
        Column::new("Quant").pad(8).droppable(10),
        Column::new("Size").pad(10).droppable(9),
        Column::new("Context").pad(10).droppable(6),
        Column::new("Images").pad(7).droppable(3),
        // As wide as its widest entry: the roles an image model needs are
        // never cut.
        Column::new("Draws").droppable(4),
        Column::new("Added").pad(20).droppable(0),
        Column::new("File Path").shorten(Shorten::Path).droppable(8),
    ]);
    columns
}

/// The table, as text, fitted to `width` or whole when there is none.
fn render_table(models: &[GuiModel], width: Option<usize>) -> String {
    // The column shows when a row has a speed to print, not merely a summary:
    // a library benchmarked only by compare runs has summaries without one.
    let show_speed = models.iter().any(|m| latest_speed(m).is_some());
    let mut table = Table::new(columns(show_speed)).ruler('-');
    for model in models {
        let context = model
            .context_length
            .map_or_else(|| "--".to_string(), |c| c.to_string());
        // What it takes on disk, all its files; `--` when none could be read.
        let size = if model.size_bytes == 0 {
            "--".to_owned()
        } else {
            format_size(model.size_bytes)
        };
        let mut row = vec![
            model.id.to_string(),
            truncate_string(&model.name, 24),
            model.openai_id.clone().unwrap_or_else(|| "--".to_owned()),
            format_params(model.param_count_b),
        ];
        if show_speed {
            let speed = latest_speed(model).map_or_else(|| "--".to_string(), |t| format!("{t:.1}"));
            row.push(truncate_string(&speed, 9));
        }
        row.extend([
            truncate_string(model.architecture.as_deref().unwrap_or("--"), 11),
            truncate_string(model.quantization.as_deref().unwrap_or("--"), 7),
            size,
            truncate_string(&context, 9),
            // A model reads images exactly when it is linked to a projector.
            if model.image_input { "yes" } else { "--" }.to_owned(),
            draws(model),
            model.added_at.clone(),
            model.file_path.clone(),
        ]);
        table.row(row);
    }
    table.lay_out(width).text()
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
