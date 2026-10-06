//! `gglib model list --remote`: the paired machine's catalogue, as its proxy
//! publishes it.
//!
//! A `#[path]` child of `list.rs`. What is listed is that machine's
//! `GET /v1/models`, read through the tunnel by this machine's daemon — the
//! same answer any client of that machine gets — so the columns are the ones
//! that answer carries: the model's id there, the name it goes by, the
//! context it would be served with, whether it reads images, and the
//! profiles it can be asked for with. The sort and filter flags describe
//! this machine's catalogue and are not applied; the far list is short and
//! arrives sorted as that machine sorts it.
//!
//! Without `--remote`, the local list ends with [`summary`]'s one line about
//! the paired machine, which asks that machine nothing.

use std::fmt::Write as _;

use anyhow::Result;
use gglib_app_services::RemoteConnection;
use gglib_core::domain::UNNAMED_PAIRED;
use gglib_proxy::models::{ModelInfo, VISION_CAPABILITY};

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle, DaemonProbe};
use crate::handlers::remote::for_how_long;
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

/// The one line `gglib model list` ends with while this machine is paired:
/// how the paired machine stands, from what this machine already knows —
/// the stored pairing and the daemon's report of its connection. Nothing is
/// asked of the paired machine, so a local listing never waits on a tunnel;
/// it lists none of that machine's models, and names the command that does.
/// `None` when nothing is paired.
pub(super) async fn summary(ctx: &CliContext) -> Option<String> {
    let daemon = Daemon {
        ctx,
        client: gglib_proxy::loopback::client(),
    };
    summary_from(ctx, &daemon).await
}

/// What [`summary`] may ask this machine's daemon: whether it is running,
/// and its report of the connection to the paired machine. Nothing here
/// reaches that machine.
trait ConnectionReport: Sync {
    /// Whether the daemon answers its probe.
    fn running(&self) -> impl Future<Output = bool> + Send;
    /// The connection the daemon reports, if it reports one.
    fn connection(&self) -> impl Future<Output = Option<RemoteConnection>> + Send;
}

/// This machine's daemon, over loopback.
struct Daemon<'a> {
    ctx: &'a CliContext,
    client: reqwest::Client,
}

impl ConnectionReport for Daemon<'_> {
    async fn running(&self) -> bool {
        matches!(
            daemon_client::probe(&self.client).await,
            DaemonProbe::Running
        )
    }

    async fn connection(&self) -> Option<RemoteConnection> {
        let handle = DaemonHandle::new(self.ctx, self.client.clone()).await;
        handle.remote_status().await.ok().and_then(|s| s.connected)
    }
}

/// [`summary`], asking `daemon`: the stored pairing first, and the daemon
/// only while there is one; its report only once it is running.
async fn summary_from(ctx: &CliContext, daemon: &impl ConnectionReport) -> Option<String> {
    let pairing = ctx.app.settings().get().await.ok()?.remote_pairing?;
    let connection = if daemon.running().await {
        daemon.connection().await
    } else {
        None
    };
    Some(summary_line(pairing.name.as_deref(), connection.as_ref()))
}

/// [`summary`]'s line, given the name the stored pairing has for the paired
/// machine and the connection the daemon reported, if it reported one. One
/// command per sentence, each to type as it stands.
fn summary_line(name: Option<&str>, connection: Option<&RemoteConnection>) -> String {
    let name = name.unwrap_or(UNNAMED_PAIRED);
    match connection.map(|c| (c.away_for_s, &c.path)) {
        None => format!("{name}: not connected"),
        Some((Some(secs), _)) => format!("{name}: away {}", for_how_long(secs)),
        Some((None, path)) => {
            format!("Paired with {name} ({path}). Its models: gglib model list --remote")
        }
    }
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
    /// Whether that machine lists it with the `vision` capability.
    images: bool,
    profiles: Vec<&'a str>,
}

/// One row per `gglib_id`, in the order the far machine listed them: the
/// base entry's name and context, each variant's profile, and image input
/// when any of its entries lists it.
fn rows(models: &[ModelInfo]) -> Vec<Row<'_>> {
    let mut rows: Vec<Row<'_>> = Vec::new();
    for model in models {
        if !rows.iter().any(|row| row.id == model.gglib_id) {
            rows.push(Row {
                id: model.gglib_id,
                name: base_name(model),
                context: None,
                images: false,
                profiles: Vec::new(),
            });
        }
        let Some(row) = rows.iter_mut().find(|row| row.id == model.gglib_id) else {
            continue;
        };
        row.images |= sees(model);
        if let Some(profile) = model.profile.as_deref() {
            row.profiles.push(profile);
        } else {
            row.name = &model.id;
            row.context = model.context_window;
        }
    }
    rows
}

/// Whether the far machine lists `model` as reading images: the far spelling
/// of the local list's `Images` column.
fn sees(model: &ModelInfo) -> bool {
    model
        .capabilities
        .as_deref()
        .is_some_and(|all| all.iter().any(|c| c == VISION_CAPABILITY))
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
        "{:>id_width$}  {:<name_width$}  {:>9}  {:<6}  PROFILES\n",
        "ID", "NAME", "CONTEXT", "IMAGES"
    );
    out.push_str(&"-".repeat(id_width + name_width + 31));
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
        let images = if row.images { "yes" } else { "-" };
        let _ = writeln!(
            out,
            "{id:>id_width$}  {:<name_width$}  {context:>9}  {images:<6}  {profiles}",
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
