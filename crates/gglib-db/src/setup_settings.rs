//! Settings rows whose setting is gone: one reclaimed outright, one carried
//! over into the setting that replaced it first.
//!
//! A `#[path]` child of `setup.rs`. `Settings` is `#[serde(default)]` and
//! nothing validates the key set, so a row no field answers to is passed over
//! at load and breaks nothing. Nothing sweeps it either: a write goes through
//! the fields there are, one row each.

use anyhow::Result;
use gglib_core::{LoopGuardMode, Settings};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{SqliteConnection, SqliteExecutor, SqlitePool};

use crate::repositories::sqlite_settings_repository::{now, stored_rows, values, write_row};

/// `PRAGMA user_version` once the loop guard's switch has been folded into
/// its mode.
///
/// The second version this project has assigned, above
/// [`CANONICAL_PATH_SCHEMA_VERSION`](super::CANONICAL_PATH_SCHEMA_VERSION),
/// which goes on meaning what it meant; anything later takes a higher number
/// still.
pub(super) const LOOP_GUARD_MODE_SCHEMA_VERSION: i64 = 2;

/// The key of the row that was the loop guard's switch.
const SWITCH: &str = "proxy_loop_detection";

/// The key of the row that holds the loop guard's mode.
const MODE: &str = "loop_guard_mode";

/// No setting has the key `auto_tune`; reclaim that row.
///
/// A stale row would never break anything, and would also never be swept.
/// House style reclaims dropped *tables*; an orphan key would otherwise sit in
/// every existing database forever, reading like a setting that still does
/// something.
pub(super) async fn reclaim_auto_tune(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM settings_kv WHERE key = 'auto_tune'")
        .execute(pool)
        .await?;
    Ok(())
}

/// Migration: fold the loop guard's switch into its mode, once per database.
///
/// `proxy_loop_detection` was a boolean beside `loop_guard_mode`, consulted
/// when no mode was stored: `false` answered [`LoopGuardMode::Off`], `true`
/// answered [`LoopGuardMode::Note`]. `Settings` has no such field, so a stored
/// `false` with no mode beside it would be passed over at load and the guard
/// would come back on with nothing said. [`fold`] stores what the pair
/// answered and deletes the switch.
///
/// Gated on `user_version`, so it runs once: a switch an older build writes
/// into a stamped database is not folded. A stamped database is never made to
/// wait for the write lock; only one this read finds unstamped is.
pub(super) async fn fold_loop_guard_switch_into_mode(pool: &SqlitePool) -> Result<()> {
    if user_version(pool).await? < LOOP_GUARD_MODE_SCHEMA_VERSION {
        fold_and_stamp(pool).await?;
    }
    Ok(())
}

/// [`fold`] and the stamp, in one `BEGIN IMMEDIATE` transaction.
///
/// The version is asked again once the lock is held, so of two processes
/// opening one database the second finds the work done, and a later version
/// stamped in the meantime is not lowered. Stamped whatever `fold` found.
async fn fold_and_stamp(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if user_version(&mut *tx).await? >= LOOP_GUARD_MODE_SCHEMA_VERSION {
        return Ok(());
    }
    fold(&mut tx).await?;
    // Not bindable as a parameter: SQLite requires a literal here.
    sqlx::query(&format!(
        "PRAGMA user_version = {LOOP_GUARD_MODE_SCHEMA_VERSION}"
    ))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn user_version(conn: impl SqliteExecutor<'_>) -> sqlx::Result<i64> {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(conn)
        .await
}

/// Store as the mode what the switch and the mode answered together, where
/// the mode alone would answer otherwise, and delete the switch's row.
///
/// No other row is written, and the mode's row only when its answer moves, so
/// a second pass finds no switch and writes nothing.
///
/// A record the build before this one could not read answered nothing, so
/// there is nothing to carry over: every row is left exactly as found, and
/// that is not an error. Only a failure to reach the database is.
async fn fold(conn: &mut SqliteConnection) -> Result<()> {
    let (switch, settings) = match as_last_read(&stored_rows(conn).await?) {
        Ok(Some(read)) => read,
        Ok(None) => return Ok(()),
        Err(_) => {
            // Not the reason: it can quote the value of the row it stopped at.
            tracing::warn!(
                "the stored settings cannot be read, so a retired loop guard \
                 switch among them is left as found"
            );
            return Ok(());
        }
    };

    let off = switch == Some(false);
    let answered = settings.loop_guard_mode.unwrap_or(if off {
        LoopGuardMode::Off
    } else {
        LoopGuardMode::Note
    });
    let updated_at = now();
    if answered != settings.effective_loop_guard_mode() {
        let value = serde_json::to_value(answered)?;
        write_row(conn, MODE, Some(&value), &updated_at).await?;
    }
    write_row(conn, SWITCH, None, &updated_at).await?;
    tracing::info!(mode = ?answered, "folded the loop guard's retired switch into its mode");
    Ok(())
}

/// The switch and the settings beside it, as the build before this one read
/// `rows`: `None` when they hold no switch, and why when it could not read
/// them.
///
/// That build read every row this build reads and the switch as a boolean or
/// `null`, so it could not read a record this build's own load refuses, nor
/// one whose switch is anything else.
fn as_last_read(rows: &[SqliteRow]) -> Result<Option<(Option<bool>, Settings)>> {
    let mut record = values(rows)?;
    let Some(switch) = record.remove(SWITCH) else {
        return Ok(None);
    };
    Ok(Some((
        serde_json::from_value(switch)?,
        serde_json::from_value(Value::Object(record))?,
    )))
}

#[cfg(test)]
#[path = "setup_settings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "setup_settings_unreadable_tests.rs"]
mod unreadable_tests;
