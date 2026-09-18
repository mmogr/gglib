//! `gglib proxy trips`: the loop guard's log, read from this machine's
//! database.
//!
//! Not through the daemon. The log is two tables in the database every CLI
//! run already opens, so reading them directly answers whether or not a
//! daemon is running — after a restart, which is when a log that outlives the
//! process exists to be read. The daemon's `GET /api/proxy/loop-guard-trips`
//! reads the same tables for the GUI's settings panel.
//!
//! One row per UTC day, model, gglib version and mode: the requests the guard
//! scanned, and of those the ones it acted on — noted under `note`, refused
//! under `refuse` — by detector, with how many sessions they came from. A day
//! scanned without a trip is printed with zero trips: that is the reading
//! ADR 0011's criterion asks for.

use std::io::Write;

use anyhow::Result;
use gglib_core::domain::loop_guard_log::{
    LOOP_GUARD_LOG_RETENTION_DAYS, LoopGuardTripDay, first_day_of_window,
};
use gglib_core::ports::LoopGuardTripLog;

use crate::bootstrap::CliContext;

/// Execute `gglib proxy trips`.
pub(crate) async fn execute(ctx: &CliContext, since_days: u32) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = read(ctx.loop_guard_trips.as_ref(), since_days, now).await?;
    render(&mut std::io::stdout().lock(), &days, since_days)?;
    Ok(())
}

/// The log's days in the window of `since_days` ending with the day `now`
/// falls on.
async fn read(
    log: &dyn LoopGuardTripLog,
    since_days: u32,
    now: u64,
) -> Result<Vec<LoopGuardTripDay>> {
    Ok(log.summary(first_day_of_window(now, since_days)).await?)
}

/// Print the days as a table, or say that there were none.
fn render(out: &mut impl Write, days: &[LoopGuardTripDay], since_days: u32) -> std::io::Result<()> {
    let window = since_days.clamp(1, LOOP_GUARD_LOG_RETENTION_DAYS);
    if days.is_empty() {
        writeln!(
            out,
            "The loop guard scanned nothing in the last {window} day(s), \
             or it is off (`gglib config settings set --loop-guard-mode note`)."
        )?;
        return Ok(());
    }
    writeln!(out, "The loop guard's log, the last {window} day(s):")?;
    writeln!(
        out,
        "{:<10}  {:<28}  {:<9}  {:<6}  {:>7}  {:>5}  {:>5}  {:>6}  {:>8}",
        "DAY (UTC)", "MODEL", "VERSION", "MODE", "SCANNED", "TRIPS", "LOOPS", "STAGN.", "SESSIONS"
    )?;
    for d in days {
        let mode = serde_json::to_value(d.mode)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        writeln!(
            out,
            "{:<10}  {:<28}  {:<9}  {:<6}  {:>7}  {:>5}  {:>5}  {:>6}  {:>8}",
            d.day,
            d.model_name,
            d.gglib_version,
            mode,
            d.scanned,
            d.trips,
            d.loops,
            d.stagnations,
            d.sessions
        )?;
    }
    writeln!(
        out,
        "\nA trip is the guard's decision, not a delivery: a noted request can still fail \
         to reach the model."
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "proxy_trips_tests.rs"]
mod tests;
