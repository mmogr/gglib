//! The loop guard's log tables, and step 7: the guard is on or off.
//!
//! A `#[path]` child of `setup.rs`. The guard had three modes, `off`, `note`
//! and `refuse`; it now has a switch, `loop_guard`, where no row means on and
//! "on" is what `note` did. Step 7 leaves a library with no trace of the
//! modes:
//!
//! - **Settings.** A stored `loop_guard_mode` of `"off"` becomes
//!   `loop_guard = false`; `note` and `refuse` become no row, which is on.
//!   The mode's row goes, and so do the rows of settings deleted before it
//!   that nothing reads.
//! - **The log.** `loop_guard_trips` and `loop_guard_scans` lose their `mode`
//!   column. Scans are keyed by day, model and version now, so two rows that
//!   differed only by mode are merged by adding their counts.
//! - **Eval reports.** A stored agentic report loses the proxy arm's
//!   `loop_guard_mode`.
//!
//! The statements are plain strings so they can be read as they run. An older
//! build refuses the file once it is stamped (`setup_generation.rs`), and the
//! backup taken before the upgrade (`setup_backup.rs`) is the way back.

use anyhow::Result;
use sqlx::SqliteConnection;

/// The decisions table, as a new library gets it and step 7 rebuilds it.
pub(super) const TRIPS_TABLE: &str = "CREATE TABLE IF NOT EXISTS loop_guard_trips (
            id             INTEGER PRIMARY KEY,
            recorded_at    INTEGER NOT NULL,
            model_name     TEXT    NOT NULL,
            gglib_version  TEXT    NOT NULL,
            detector       TEXT    NOT NULL,
            signature_hash TEXT,
            session_hash   TEXT,
            repeat_count   INTEGER,
            threshold      INTEGER
        )";

/// The decisions table's index, which the prune reads by.
pub(super) const TRIPS_INDEX: &str = "CREATE INDEX IF NOT EXISTS idx_loop_guard_trips_recorded_at \
     ON loop_guard_trips(recorded_at)";

/// The scans table, as a new library gets it and step 7 rebuilds it.
pub(super) const SCANS_TABLE: &str = "CREATE TABLE IF NOT EXISTS loop_guard_scans (
            day           INTEGER NOT NULL,
            model_name    TEXT    NOT NULL,
            gglib_version TEXT    NOT NULL,
            scanned       INTEGER NOT NULL,
            PRIMARY KEY (day, model_name, gglib_version)
        )";

/// Settings: `"off"` becomes the switch, then the retired rows go.
///
/// The deleted keys: the mode itself; `proxy_loop_detection`, the switch
/// before the mode (step 2 folded it); `auto_tune` (#810); `tool_call_floor`
/// (#744) and `agentic_sampling`, the deleted tool-turn temperature cap;
/// `remote_api_key` and `remote_last_ticket`, which `remote_pairing`
/// replaced; and `bind_host`, `default_download_path`, `share_lan` and
/// `show_memory_fit_indicators`, deleted with the old settings page (#1805).
const SETTINGS: [&str; 2] = [
    r#"INSERT OR REPLACE INTO settings_kv (key, value, updated_at)
    SELECT 'loop_guard', 'false', updated_at FROM settings_kv
    WHERE key = 'loop_guard_mode' AND value = '"off"'"#,
    "DELETE FROM settings_kv WHERE key IN (
    'loop_guard_mode', 'proxy_loop_detection', 'auto_tune', 'tool_call_floor',
    'agentic_sampling', 'remote_api_key', 'remote_last_ticket', 'bind_host',
    'default_download_path', 'share_lan', 'show_memory_fit_indicators'
)",
];

/// Set the decisions table aside before its rebuild. Its index goes with it.
const TRIPS_SET_ASIDE: &str = "ALTER TABLE loop_guard_trips RENAME TO loop_guard_trips_old";

/// Copy every decision across, `mode` left behind, and drop the old table.
const TRIPS_COPY: [&str; 2] = [
    "INSERT INTO loop_guard_trips (id, recorded_at, model_name, gglib_version, detector,
        signature_hash, session_hash, repeat_count, threshold)
    SELECT id, recorded_at, model_name, gglib_version, detector,
        signature_hash, session_hash, repeat_count, threshold
    FROM loop_guard_trips_old",
    "DROP TABLE loop_guard_trips_old",
];

/// Set the scans table aside before its rebuild.
const SCANS_SET_ASIDE: &str = "ALTER TABLE loop_guard_scans RENAME TO loop_guard_scans_old";

/// Copy the scans across, merging rows that differed only by mode, and drop
/// the old table.
const SCANS_COPY: [&str; 2] = [
    "INSERT INTO loop_guard_scans (day, model_name, gglib_version, scanned)
    SELECT day, model_name, gglib_version, SUM(scanned)
    FROM loop_guard_scans_old
    GROUP BY day, model_name, gglib_version",
    "DROP TABLE loop_guard_scans_old",
];

/// Eval reports: the proxy arm's mode goes.
const REPORTS: &str = "UPDATE benchmark_agentic_results
    SET report_json = json_remove(report_json, '$.proxy.settings.loop_guard_mode')
    WHERE json_valid(report_json)
    AND json_type(report_json, '$.proxy.settings.loop_guard_mode') IS NOT NULL";

/// Step 7: the loop guard is on or off. See the module docs.
///
/// A table already without `mode` — a new library's — is not rebuilt, so the
/// step is safe on any file this build made.
pub(super) async fn make_the_guard_a_switch(conn: &mut SqliteConnection) -> Result<()> {
    execute(conn, &SETTINGS).await?;
    if has_mode(conn, "loop_guard_trips").await? {
        let [copy, drop] = TRIPS_COPY;
        execute(
            conn,
            &[TRIPS_SET_ASIDE, TRIPS_TABLE, copy, drop, TRIPS_INDEX],
        )
        .await?;
    }
    if has_mode(conn, "loop_guard_scans").await? {
        let [copy, drop] = SCANS_COPY;
        execute(conn, &[SCANS_SET_ASIDE, SCANS_TABLE, copy, drop]).await?;
    }
    execute(conn, &[REPORTS]).await
}

/// Each statement, in order.
async fn execute(conn: &mut SqliteConnection, statements: &[&str]) -> Result<()> {
    for statement in statements {
        sqlx::query(statement).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Whether `table` still has the `mode` column.
async fn has_mode(conn: &mut SqliteConnection, table: &str) -> Result<bool> {
    let found: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = 'mode'")
            .bind(table)
            .fetch_one(&mut *conn)
            .await?;
    Ok(found > 0)
}

#[cfg(test)]
#[path = "setup_loop_guard_tests.rs"]
mod tests;
