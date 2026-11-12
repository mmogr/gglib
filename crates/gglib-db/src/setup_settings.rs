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
use sqlx::{SqliteConnection, SqlitePool};

use crate::repositories::sqlite_settings_repository::{now, stored_rows, values, write_row};

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

/// Step 2: fold the loop guard's switch into its mode.
///
/// `proxy_loop_detection` was a boolean beside `loop_guard_mode`, consulted
/// when no mode was stored: `false` answered [`LoopGuardMode::Off`], `true`
/// answered [`LoopGuardMode::Note`]. `Settings` has no such field, so a stored
/// `false` with no mode beside it would be passed over at load and the guard
/// would come back on with nothing said. This stores as the mode what the
/// pair answered, where the mode alone would answer otherwise, and deletes
/// the switch's row. Once stamped, a switch an older build writes is not
/// folded.
///
/// No other row is written, and the mode's row only when its answer moves, so
/// a second pass finds no switch and writes nothing.
///
/// A record the build before this one could not read answered nothing, so
/// there is nothing to carry over: every row is left exactly as found, and
/// that is not an error. Only a failure to reach the database is.
pub(super) async fn fold_loop_guard_switch_into_mode(conn: &mut SqliteConnection) -> Result<()> {
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
