//! Paths command handler.
//!
//! Displays all resolved paths for diagnostics and debugging.
//! This is the "golden truth" tool for path resolution issues.

use anyhow::Result;

use gglib_core::paths::ResolvedPaths;

/// Execute the paths command.
///
/// Resolves and displays all paths used by gglib in `key = value` format.
/// This is useful for debugging path resolution issues and verifying
/// that all adapters (CLI, GUI, Web) use the same paths.
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
pub fn execute() -> Result<()> {
    let paths = ResolvedPaths::resolve()?;
    println!("{paths}");
    Ok(())
}
