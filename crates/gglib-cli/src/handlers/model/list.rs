//! List command handler.
//!
//! Fetches and displays GGUF models with optional sort / filter flags. Models
//! are loaded from the local `SQLite` database and filtered in-process via
//! [`gglib_core::domain::apply_query`], into a `Vec<GuiModel>` that is
//! rendered by a single table function. A speed column (`⚡ t/s`) is shown
//! only when at least one returned model has benchmark data. While this
//! machine is paired, the list ends with one line on how the paired machine
//! stands, which asks that machine nothing.

use std::fmt::Write as _;

use anyhow::Result;
use gglib_app_services::types::GuiModel;
use gglib_core::domain::{ModelListQuery, apply_query};

use crate::bootstrap::CliContext;
use crate::model_commands::{CliModelSortBy, CliSortOrder};
use crate::presentation::truncate_string;
use crate::target::Target;

#[path = "list_far.rs"]
mod list_far;

// ─────────────────────────────────────────────────────────────────────────────
// Public surface
// ─────────────────────────────────────────────────────────────────────────────

/// Arguments forwarded from the `List` CLI variant.
pub(crate) struct ListArgs {
    pub sort: CliModelSortBy,
    pub order: CliSortOrder,
    pub min_params: Option<f64>,
    pub max_params: Option<f64>,
    pub min_speed: Option<f64>,
    pub max_speed: Option<f64>,
    pub tags: Vec<String>,
}

/// Execute the list command: this machine's catalogue, or the paired
/// machine's as its proxy publishes it.
pub(crate) async fn execute(target: Target, ctx: &CliContext, args: ListArgs) -> Result<()> {
    target
        .run(
            async || list_here(ctx, args).await,
            async || list_far::execute(ctx, target).await,
        )
        .await
}

/// This machine's catalogue, sorted and filtered as asked, and the paired
/// machine's line when there is one.
async fn list_here(ctx: &CliContext, args: ListArgs) -> Result<()> {
    let models = fetch_models(ctx, &args).await?;
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

async fn fetch_models(ctx: &CliContext, args: &ListArgs) -> Result<Vec<GuiModel>> {
    let query = build_query(args);
    let all = ctx.app.models().list().await?;
    let filtered = apply_query(all, &query);
    Ok(filtered.into_iter().map(GuiModel::from_domain).collect())
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

/// The table, as text. The ID column is as wide as the widest id, so a
/// four-digit id does not push its row out of line with the rest.
fn render_table(models: &[GuiModel]) -> String {
    let show_speed = models.iter().any(|m| m.benchmark_summary.is_some());
    let id_width = models
        .iter()
        .map(|m| m.id.to_string().len())
        .max()
        .unwrap_or(0)
        .max(3);
    let mut out = String::new();

    if show_speed {
        let _ = writeln!(
            out,
            "{:<id_width$} {:<25} {:<8} {:<10} {:<12} {:<8} {:<10} {:<20} File Path",
            "ID", "Name", "Params", "⚡ t/s", "Arch", "Quant", "Context", "Added"
        );
        let _ = writeln!(out, "{}", "-".repeat(125 + id_width));
    } else {
        let _ = writeln!(
            out,
            "{:<id_width$} {:<25} {:<8} {:<12} {:<8} {:<10} {:<20} File Path",
            "ID", "Name", "Params", "Arch", "Quant", "Context", "Added"
        );
        let _ = writeln!(out, "{}", "-".repeat(112 + id_width));
    }

    for model in models {
        let arch = model.architecture.as_deref().unwrap_or("--");
        let quant = model.quantization.as_deref().unwrap_or("--");
        let context = model
            .context_length
            .map_or_else(|| "--".to_string(), |c| c.to_string());

        if show_speed {
            let speed = model
                .benchmark_summary
                .as_ref()
                .and_then(|s| s.latest_tg_tps)
                .map_or_else(|| "--".to_string(), |t| format!("{t:.1}"));
            let _ = writeln!(
                out,
                "{:<id_width$} {:<25} {:<8.1} {:<10} {:<12} {:<8} {:<10} {:<20} {}",
                model.id,
                truncate_string(&model.name, 24),
                model.param_count_b,
                truncate_string(&speed, 9),
                truncate_string(arch, 11),
                truncate_string(quant, 7),
                truncate_string(&context, 9),
                model.added_at,
                model.file_path,
            );
        } else {
            let _ = writeln!(
                out,
                "{:<id_width$} {:<25} {:<8.1} {:<12} {:<8} {:<10} {:<20} {}",
                model.id,
                truncate_string(&model.name, 24),
                model.param_count_b,
                truncate_string(arch, 11),
                truncate_string(quant, 7),
                truncate_string(&context, 9),
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
