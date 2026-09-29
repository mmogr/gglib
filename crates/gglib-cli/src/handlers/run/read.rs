//! `gglib run list`, `show` and `cancel`.

use std::io::Write as _;
use std::ops::ControlFlow;

use anyhow::{Result, bail};
use gglib_core::domain::runs::{RunInfo, RunStatus};

use super::text::{delta_text, past_snapshot};
use crate::daemon_client::DaemonHandle;
use crate::daemon_client::runs::RunItem;

/// A status as a person reads it.
const fn word(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::InProgress => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
    }
}

/// Milliseconds since the epoch as a local time of day.
fn clock(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map_or_else(String::new, |t| {
            t.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
}

/// `gglib run list`.
pub(super) async fn list(daemon: &DaemonHandle) -> Result<()> {
    let runs = daemon.run_list().await?.runs;
    if runs.is_empty() {
        eprintln!("  No runs. `gglib run start --model <m> \"…\"` starts one.");
        return Ok(());
    }
    println!(
        "{:<18} {:<10} {:<9} {:>6}  MODEL",
        "ID", "STATUS", "STARTED", "EVENTS"
    );
    for run in &runs {
        let model = run.model.as_deref().unwrap_or("-");
        let from = run
            .device
            .as_deref()
            .map_or_else(String::new, |d| format!("  (from {d})"));
        println!(
            "{:<18} {:<10} {:<9} {:>6}  {model}{from}",
            run.id,
            word(run.status),
            clock(run.created_at_ms),
            run.last_seq
        );
    }
    Ok(())
}

/// `gglib run show`.
pub(super) async fn show(daemon: &DaemonHandle, id: &str, follow: bool) -> Result<()> {
    let info = daemon.run_get(id).await?;
    if let Some(device) = &info.device {
        eprintln!(
            "  {id} is {} and belongs to {device}; its reply is the device's to read.",
            word(info.status)
        );
        return Ok(());
    }
    if !follow && info.last_seq == 0 {
        return finish(id, &info, None);
    }
    print_reply(daemon, id, follow, info.last_seq).await
}

/// Print a run's reply text, to its end with `follow`, else up to `last_seq`.
pub(super) async fn print_reply(
    daemon: &DaemonHandle,
    id: &str,
    follow: bool,
    last_seq: u32,
) -> Result<()> {
    let mut out = std::io::stdout();
    let end = daemon
        .run_events(id, 0, |item| {
            let RunItem::Frame { seq, data } = item else {
                return ControlFlow::Continue(());
            };
            if let Some(text) = delta_text(data) {
                let _ = write!(out, "{text}");
                let _ = out.flush();
            }
            if past_snapshot(follow, last_seq, *seq) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .await?;
    println!();
    match end {
        Some(info) => finish(id, &info, None),
        None => finish(id, &daemon.run_get(id).await?, Some(follow)),
    }
}

/// Say how the run stands. A failed run is an error, so the exit code says
/// so too.
fn finish(id: &str, info: &RunInfo, stopped_early: Option<bool>) -> Result<()> {
    match info.status {
        RunStatus::Failed => {
            let (code, message) = info
                .error
                .as_ref()
                .map_or(("unknown", "no reason given"), |e| {
                    (e.code.as_str(), e.message.as_str())
                });
            bail!("run {id} failed ({code}): {message}")
        }
        status if status.is_terminal() => {
            eprintln!("  run {id} {}", word(status));
        }
        status => {
            eprintln!(
                "  run {id} is still {}{}",
                word(status),
                if stopped_early == Some(true) {
                    "; the stream closed before it ended"
                } else {
                    ". `gglib run show <id> --follow` keeps reading"
                }
            );
        }
    }
    Ok(())
}

/// `gglib run cancel`.
pub(super) async fn cancel(daemon: &DaemonHandle, id: &str) -> Result<()> {
    let info = daemon.run_cancel(id).await?;
    if info.status == RunStatus::Cancelled {
        eprintln!("  Cancelled {id}.");
    } else {
        eprintln!("  {id} had already ended: {}.", word(info.status));
    }
    Ok(())
}
