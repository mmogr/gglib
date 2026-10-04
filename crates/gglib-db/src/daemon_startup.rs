//! What the daemon puts right in the database when it starts.
//!
//! These run at daemon start and not when the CLI opens the database: the
//! daemon is the one long-lived process, started once, and each repair is
//! safe only then. A failure is logged and the daemon starts anyway.

use sqlx::SqlitePool;

use crate::setup::{cleanup_zombie_benchmark_runs, sweep_unlinked_attachments};

/// Run each startup repair once, after the schema is ready.
pub async fn repair_at_daemon_start(pool: &SqlitePool) {
    // A benchmark run left `running` by a crash is marked failed. Only the
    // daemon can take it that no live process owns such a row.
    if let Err(e) = cleanup_zombie_benchmark_runs(pool).await {
        tracing::warn!("Failed to clean up zombie benchmark runs on startup: {e}");
    }

    // An image no message carries, last stored more than a day ago, is
    // deleted. A newer one may belong to a turn in flight: an image is
    // stored before its message is saved, and by the CLI in its own process.
    match sweep_unlinked_attachments(pool).await {
        Ok(0) => {}
        Ok(swept) => tracing::info!(swept, "deleted old stored images no message carries"),
        Err(e) => tracing::warn!("Failed to delete unlinked images on startup: {e}"),
    }
}

#[cfg(test)]
#[path = "daemon_startup_tests.rs"]
mod daemon_startup_tests;
