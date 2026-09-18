//! `SQLite` implementation of [`LoopGuardTripLog`], and the statements the
//! [`LoopGuardTripWriter`](crate::LoopGuardTripWriter) flushes and prunes
//! through.
//!
//! Two tables. `loop_guard_trips` holds one row per decision the guard took.
//! `loop_guard_scans` holds one row per UTC day, model, gglib version and
//! mode, counting the requests the guard scanned — the denominator. The
//! summary has a row for every key **either** table holds: a day the guard
//! scanned and never tripped comes back with `trips` at zero rather than not
//! at all — the reading ADR 0011's criterion asks for — and a trip whose scan
//! was lost in a refused flush still shows, with `scanned` at zero, rather
//! than vanishing.

use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::{Row, SqliteConnection, SqlitePool};

use gglib_core::LoopGuardMode;
use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::domain::loop_guard_log::{LoopGuardTripDay, LoopGuardTripEvent, epoch_day};
use gglib_core::ports::{LoopGuardTripLog, RepositoryError};

/// What one day's scans are counted under: the epoch day, the bounded model
/// name and the mode. The gglib version is the writer's own, added at flush.
pub(crate) type ScanKey = (i64, String, LoopGuardMode);

const SECS_PER_DAY: i64 = 86_400;

/// `SQLite` implementation of [`LoopGuardTripLog`].
pub struct SqliteLoopGuardTripLog {
    pool: SqlitePool,
}

impl SqliteLoopGuardTripLog {
    /// Create a reader over a shared connection pool.
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const fn detector_str(detector: LoopGuardTrip) -> &'static str {
    match detector {
        LoopGuardTrip::Loop => "loop",
        LoopGuardTrip::Stagnation => "stagnation",
    }
}

const fn mode_str(mode: LoopGuardMode) -> &'static str {
    match mode {
        LoopGuardMode::Off => "off",
        LoopGuardMode::Note => "note",
        LoopGuardMode::Refuse => "refuse",
    }
}

fn parse_mode(text: &str) -> Result<LoopGuardMode, RepositoryError> {
    match text {
        "note" => Ok(LoopGuardMode::Note),
        "refuse" => Ok(LoopGuardMode::Refuse),
        "off" => Ok(LoopGuardMode::Off),
        other => Err(RepositoryError::Serialization(format!(
            "unknown loop guard mode in the log: {other}"
        ))),
    }
}

fn storage(e: &sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}

fn count(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<u64, RepositoryError> {
    let n: i64 = row.try_get(column).map_err(|e| storage(&e))?;
    u64::try_from(n).map_err(|_| RepositoryError::Serialization(format!("negative {column}")))
}

/// Write one flush: every trip as a row of its own, and every day's scans added
/// onto what that day already counts.
pub(crate) async fn write_batch(
    conn: &mut SqliteConnection,
    version: &str,
    trips: &[LoopGuardTripEvent],
    scans: &HashMap<ScanKey, u64>,
) -> Result<(), sqlx::Error> {
    for trip in trips {
        sqlx::query(
            "INSERT INTO loop_guard_trips (recorded_at, model_name, gglib_version, mode, \
             detector, signature_hash, session_hash, repeat_count, threshold) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(i64::try_from(trip.recorded_at_secs()).unwrap_or(i64::MAX))
        .bind(trip.model_name())
        .bind(version)
        .bind(mode_str(trip.mode()))
        .bind(detector_str(trip.detector()))
        .bind(trip.signature_hash())
        .bind(trip.session_hash())
        .bind(trip.repeat_count())
        .bind(trip.threshold())
        .execute(&mut *conn)
        .await?;
    }
    for ((day, model_name, mode), scanned) in scans {
        sqlx::query(
            "INSERT INTO loop_guard_scans (day, model_name, gglib_version, mode, scanned) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(day, model_name, gglib_version, mode) \
             DO UPDATE SET scanned = scanned + excluded.scanned",
        )
        .bind(day)
        .bind(model_name)
        .bind(version)
        .bind(mode_str(*mode))
        .bind(i64::try_from(*scanned).unwrap_or(i64::MAX))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Drop what the log no longer keeps.
///
/// Every day before the last `retention_days`, from both tables. Then, if the
/// trips outnumber `row_cap`, **whole days**, oldest first, from both tables:
/// cutting a day's trips and keeping its scans would leave a day that reads
/// "scanned, never tripped", which is exactly the false signal the log exists
/// to rule out. Today is never cut by the cap, so a runaway day stays whole
/// until it is over — and if today alone holds more trips than the cap, every
/// earlier day goes. The cap counts trip rows; scan rows, one per day, model,
/// version and mode (a client-chosen model name included), are bounded by the
/// retention alone.
pub(crate) async fn prune(
    conn: &mut SqliteConnection,
    now_secs: u64,
    retention_days: u32,
    row_cap: u32,
) -> Result<(), sqlx::Error> {
    let today = epoch_day(now_secs);
    let keep_from = today - i64::from(retention_days.max(1)) + 1;
    delete_days_before(conn, keep_from).await?;

    let newest_over_cap: Option<i64> = sqlx::query_scalar(
        "SELECT recorded_at / 86400 FROM loop_guard_trips \
         ORDER BY recorded_at DESC LIMIT 1 OFFSET ?",
    )
    .bind(row_cap)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(day) = newest_over_cap {
        delete_days_before(conn, day.min(today - 1) + 1).await?;
    }
    Ok(())
}

/// Delete every trip and every scan dated before `first_kept_day`.
async fn delete_days_before(
    conn: &mut SqliteConnection,
    first_kept_day: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM loop_guard_trips WHERE recorded_at < ?")
        .bind(first_kept_day.saturating_mul(SECS_PER_DAY))
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM loop_guard_scans WHERE day < ?")
        .bind(first_kept_day)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[async_trait]
impl LoopGuardTripLog for SqliteLoopGuardTripLog {
    async fn summary(&self, first_day: i64) -> Result<Vec<LoopGuardTripDay>, RepositoryError> {
        let rows = sqlx::query(
            "WITH t AS ( \
                 SELECT recorded_at / 86400 AS day, model_name, gglib_version, mode, \
                        COUNT(*) AS trips, \
                        SUM(detector = 'loop') AS loops, \
                        SUM(detector = 'stagnation') AS stagnations, \
                        COUNT(DISTINCT session_hash) AS sessions \
                 FROM loop_guard_trips WHERE recorded_at >= ? \
                 GROUP BY 1, 2, 3, 4 \
             ), s AS ( \
                 SELECT day, model_name, gglib_version, mode, scanned \
                 FROM loop_guard_scans WHERE day >= ? \
             ), k AS ( \
                 SELECT day, model_name, gglib_version, mode FROM s \
                 UNION SELECT day, model_name, gglib_version, mode FROM t \
             ) \
             SELECT date(k.day * 86400, 'unixepoch') AS day, k.model_name, k.gglib_version, \
                    k.mode, COALESCE(s.scanned, 0) AS scanned, \
                    COALESCE(t.trips, 0) AS trips, COALESCE(t.loops, 0) AS loops, \
                    COALESCE(t.stagnations, 0) AS stagnations, \
                    COALESCE(t.sessions, 0) AS sessions \
             FROM k \
             LEFT JOIN s USING (day, model_name, gglib_version, mode) \
             LEFT JOIN t USING (day, model_name, gglib_version, mode) \
             ORDER BY k.day DESC, k.model_name, k.gglib_version, k.mode",
        )
        .bind(first_day.saturating_mul(SECS_PER_DAY))
        .bind(first_day)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| storage(&e))?;

        rows.iter()
            .map(|row| {
                let mode: String = row.try_get("mode").map_err(|e| storage(&e))?;
                Ok(LoopGuardTripDay {
                    day: row.try_get("day").map_err(|e| storage(&e))?,
                    model_name: row.try_get("model_name").map_err(|e| storage(&e))?,
                    gglib_version: row.try_get("gglib_version").map_err(|e| storage(&e))?,
                    mode: parse_mode(&mode)?,
                    scanned: count(row, "scanned")?,
                    trips: count(row, "trips")?,
                    loops: count(row, "loops")?,
                    stagnations: count(row, "stagnations")?,
                    sessions: count(row, "sessions")?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "sqlite_loop_guard_trip_log_tests.rs"]
mod tests;
