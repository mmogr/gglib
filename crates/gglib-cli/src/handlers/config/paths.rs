//! Paths command handler.
//!
//! Displays all resolved paths for diagnostics and debugging.
//! This is the "golden truth" tool for path resolution issues.

use anyhow::Result;

use gglib_core::paths::{ResolvedPaths, data_root_source};

/// Execute the paths command.
///
/// Prints which `gglib` is running and where its data root came from, then
/// every resolved path, in `key = value` format. The first lines answer the
/// question a machine with a packaged install and a build from a checkout
/// raises: which binary is this, and is it on the default data root or one
/// named by `GGLIB_DATA_DIR`.
pub(crate) fn execute() -> Result<()> {
    let binary = std::env::current_exe()
        .map_or_else(|e| format!("unknown ({e})"), |p| p.display().to_string());
    println!("binary = {binary}");
    println!("version = {}", gglib_build_info::LONG_VERSION);
    println!("data_root_source = {}", data_root_source());
    println!("daemon_port = {}", gglib_core::daemon_port());
    let paths = ResolvedPaths::resolve()?;
    println!("{paths}");
    Ok(())
}
