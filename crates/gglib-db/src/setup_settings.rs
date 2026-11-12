//! Step 2: the loop guard's old switch, folded into the mode that replaced
//! it.
//!
//! A `#[path]` child of `setup.rs`. `proxy_loop_detection` was a boolean
//! beside `loop_guard_mode`, consulted when no mode was stored: `false`
//! answered `off`, anything else `note`. This stores `off` as the mode where
//! the pair answered it and the mode alone would not, and deletes the switch.
//! Step 7 then turns the mode into today's `loop_guard` switch, so a library
//! old enough to take both steps keeps a guard it had switched off.
//!
//! Plain SQL on the stored JSON text, which the build that wrote these rows
//! wrote canonically: `false`, `null`, `"off"`. A value spelled any other way
//! is not read as `false`, and its guard comes back on.

use anyhow::Result;
use sqlx::SqliteConnection;

/// Write `"off"` as the mode where the switch is `false` and no mode is
/// stored (a `null` mode is none), then delete the switch.
const FOLD: [&str; 2] = [
    r#"INSERT OR REPLACE INTO settings_kv (key, value, updated_at)
    SELECT 'loop_guard_mode', '"off"', updated_at FROM settings_kv
    WHERE key = 'proxy_loop_detection' AND value = 'false'
    AND NOT EXISTS (
        SELECT 1 FROM settings_kv WHERE key = 'loop_guard_mode' AND value <> 'null'
    )"#,
    "DELETE FROM settings_kv WHERE key = 'proxy_loop_detection'",
];

/// Step 2: fold the loop guard's switch into its mode. A second pass finds
/// no switch and writes nothing.
pub(super) async fn fold_loop_guard_switch_into_mode(conn: &mut SqliteConnection) -> Result<()> {
    for statement in FOLD {
        sqlx::query(statement).execute(&mut *conn).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_settings_tests.rs"]
mod tests;
