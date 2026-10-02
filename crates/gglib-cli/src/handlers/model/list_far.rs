//! `gglib model list --remote`: the paired machine's catalogue, as its proxy
//! publishes it.
//!
//! A `#[path]` child of `list.rs`. What is listed is that machine's
//! `GET /v1/models`, read through the tunnel by this machine's daemon — the
//! same answer any client of that machine gets — so the columns are the ones
//! that answer carries: the model's id there, the name it goes by, the
//! context it would be served with, and the profiles it can be asked for
//! with. The sort and filter flags describe this machine's catalogue and are
//! not applied; the far list is short and arrives sorted as that machine
//! sorts it.

use std::fmt::Write as _;

use anyhow::Result;
use gglib_proxy::models::ModelInfo;

use crate::bootstrap::CliContext;
use crate::target::Target;

pub(super) async fn execute(ctx: &CliContext, target: Target) -> Result<()> {
    let paired = target.paired(ctx).await?;
    let listed = paired.handle.paired_models().await?;
    let rows = rows(&listed.models);
    print!(
        "{}",
        heading(rows.len(), &paired.name, &paired.connection.path)
    );
    if rows.is_empty() {
        return Ok(());
    }
    print!("{}", render(&rows));
    print!("{}", footer(&rows));
    Ok(())
}

/// The line above the table, naming the machine by the name every surface
/// shows it by, or the line that stands in for an empty one.
fn heading(count: usize, machine: &str, path: &str) -> String {
    if count == 0 {
        format!("No models on {machine}.\n")
    } else {
        format!("{count} model(s) on {machine} ({path}):\n\n")
    }
}

/// One model on the far machine, with its profile variants folded in.
#[derive(Debug, PartialEq, Eq)]
struct Row<'a> {
    id: i64,
    name: &'a str,
    context: Option<u64>,
    profiles: Vec<&'a str>,
}

/// One row per `gglib_id`, in the order the far machine listed them: the
/// base entry's name and context, and each variant's profile.
fn rows(models: &[ModelInfo]) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();
    for model in models {
        if !rows.iter().any(|row| row.id == model.gglib_id) {
            rows.push(Row {
                id: model.gglib_id,
                name: base_name(model),
                context: None,
                profiles: Vec::new(),
            });
        }
        let Some(row) = rows.iter_mut().find(|row| row.id == model.gglib_id) else {
            continue;
        };
        if let Some(profile) = model.profile.as_deref() {
            row.profiles.push(profile);
        } else {
            row.name = &model.id;
            row.context = model.context_window;
        }
    }
    rows
}

/// The name a variant's base goes by: its `id` without the `:{profile}`.
fn base_name(model: &ModelInfo) -> &str {
    model
        .profile
        .as_deref()
        .and_then(|profile| model.id.strip_suffix(profile))
        .and_then(|rest| rest.strip_suffix(':'))
        .unwrap_or(&model.id)
}

/// The table, as text, so it can be checked without a machine.
fn render(rows: &[Row<'_>]) -> String {
    let ids: Vec<String> = rows.iter().map(|row| row.id.to_string()).collect();
    let id_width = ids.iter().map(String::len).max().unwrap_or(0).max(2);
    let name_width = rows
        .iter()
        .map(|row| row.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let mut out = format!(
        "{:>id_width$}  {:<name_width$}  {:>9}  PROFILES\n",
        "ID", "NAME", "CONTEXT"
    );
    out.push_str(&"-".repeat(id_width + name_width + 23));
    out.push('\n');
    for (row, id) in rows.iter().zip(&ids) {
        let context = row
            .context
            .map_or_else(|| "-".to_owned(), |c| c.to_string());
        let profiles = if row.profiles.is_empty() {
            "-".to_owned()
        } else {
            row.profiles.join(", ")
        };
        let _ = writeln!(
            out,
            "{id:>id_width$}  {:<name_width$}  {context:>9}  {profiles}",
            row.name
        );
    }
    out
}

/// How to send a turn to one of them: by its id, and with a profile when
/// any are listed. One command per line, each one to type.
fn footer(rows: &[Row<'_>]) -> String {
    let mut out = "\nChat with one: gglib chat <id> --remote\n".to_owned();
    if rows.iter().any(|row| !row.profiles.is_empty()) {
        out.push_str("With a profile: gglib chat <id>:<profile> --remote\n");
    }
    out
}

#[cfg(test)]
#[path = "list_far_tests.rs"]
mod tests;
