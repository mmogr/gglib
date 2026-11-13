//! Paths command handler.
//!
//! Displays all resolved paths for diagnostics and debugging.
//! This is the "golden truth" tool for path resolution issues.

use anyhow::Result;

use gglib_core::paths::{ResolvedPaths, data_root_source};

use super::{copies, copy_version};

/// Execute the paths command.
///
/// Prints which `gglib` is running and where its data root came from, then
/// every resolved path, in `key = value` format. The first lines answer the
/// question a machine with a packaged install and a build from a checkout
/// raises: which binary is this, and is it on the default data root or one
/// named by `GGLIB_DATA_DIR`. Then the database's backup from before an
/// upgrade, when there is one. Last, every other gglib a user might run
/// here, each `gglib` on `PATH` and the desktop app, which all share this
/// data root (see [`copies`]).
pub(crate) fn execute() -> Result<()> {
    let binary = std::env::current_exe()
        .map_or_else(|e| format!("unknown ({e})"), |p| p.display().to_string());
    println!("binary = {binary}");
    println!("version = {}", gglib_build_info::LONG_VERSION);
    println!("data_root_source = {}", data_root_source());
    println!("daemon_port = {}", gglib_core::daemon_port());
    let paths = ResolvedPaths::resolve()?;
    println!("{paths}");
    if let Some((_, backup)) = gglib_db::backup_beside(&paths.database_path) {
        println!("database_backup = {}", backup.display());
    }
    let read = |bin: &std::path::Path| copy_version::read_version(bin, copy_version::VERSION_WAIT);
    for line in copies::lines(&copies::Search::here(), &read) {
        println!("{line}");
    }
    Ok(())
}
