//! `gglib proxy trips`: the loop guard's log, read from this machine's
//! database.
//!
//! Not through the daemon. The log is two tables in the database every CLI
//! run already opens, so reading them directly answers whether or not a
//! daemon is running — after a restart, which is when a log that outlives the
//! process exists to be read. The daemon's `GET /api/proxy/loop-guard-trips`
//! reads the same tables for the GUI's settings panel.
//!
//! One row per UTC day, model and gglib version: the requests the guard
//! scanned, and the ones it acted on — noted, or ended — by detector, with how many sessions they came from. The ones
//! acted on are ordinarily among the requests scanned, but one whose scan was
//! lost is not, so a row can show more trips than scans. A day scanned without
//! a trip is printed with zero trips: that is the reading ADR 0011's criterion
//! asks for.

use std::io::Write;

use anyhow::Result;
use gglib_core::domain::loop_guard_log::{
    LOOP_GUARD_LOG_RETENTION_DAYS, LoopGuardTripDay, first_day_of_window,
};
use gglib_db::SqliteLoopGuardTripLog;

use crate::bootstrap::CliContext;
use crate::presentation::table_layout::{Column, Shorten, Table};
use crate::presentation::terminal::stdout_width;

/// Execute `gglib proxy trips`.
pub(crate) async fn execute(ctx: &CliContext, since_days: u32) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = read(&ctx.loop_guard_trips, since_days, now).await?;
    render(
        &mut std::io::stdout().lock(),
        &days,
        since_days,
        stdout_width(),
    )?;
    Ok(())
}

/// The log's days in the window of `since_days` ending with the day `now`
/// falls on.
async fn read(
    log: &SqliteLoopGuardTripLog,
    since_days: u32,
    now: u64,
) -> Result<Vec<LoopGuardTripDay>> {
    Ok(log.summary(first_day_of_window(now, since_days)).await?)
}

/// Print the days as a table fitted to `width`, or whole when there is
/// none, or say that there were none.
fn render(
    out: &mut impl Write,
    days: &[LoopGuardTripDay],
    since_days: u32,
    width: Option<usize>,
) -> std::io::Result<()> {
    let window = since_days.clamp(1, LOOP_GUARD_LOG_RETENTION_DAYS);
    if days.is_empty() {
        writeln!(
            out,
            "The loop guard scanned nothing in the last {window} day(s), \
             or it is off (`gglib config set loop-guard true`)."
        )?;
        return Ok(());
    }
    // MODEL and VERSION are as wide as their widest value, so an rc version
    // or a long model name never pushes its row out of line. In a narrow
    // terminal the day, the model, the trips and the scans stay longest.
    let mut table = Table::new(vec![
        Column::new("DAY (UTC)").pad(10),
        Column::new("MODEL").shorten(Shorten::End { min: 12 }),
        Column::new("VERSION").droppable(1),
        Column::new("SCANNED").right().pad(7).droppable(5),
        Column::new("TRIPS").right().pad(5).droppable(6),
        Column::new("LOOPS").right().pad(5).droppable(4),
        Column::new("STAGN.").right().pad(6).droppable(3),
        Column::new("SESSIONS").right().pad(8).droppable(2),
    ])
    .gap(2);
    for d in days {
        table.row(vec![
            d.day.clone(),
            d.model_name.clone(),
            d.gglib_version.clone(),
            d.scanned.to_string(),
            d.trips.to_string(),
            d.loops.to_string(),
            d.stagnations.to_string(),
            d.sessions.to_string(),
        ]);
    }
    writeln!(out, "The loop guard's log, the last {window} day(s):")?;
    write!(out, "{}", table.lay_out(width).text())?;
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
