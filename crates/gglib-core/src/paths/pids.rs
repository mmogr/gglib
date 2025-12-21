//! PID file directory path resolution.
//!
//! Provides the canonical location for storing process ID files to track
//! running llama-server instances.

use std::path::PathBuf;

use super::PathError;
use super::platform::data_root;

/// Returns the directory where PID files are stored.
///
/// Location: `~/.gglib/pids/` (or equivalent data root)
///
/// This directory is used to track running llama-server processes across
/// application restarts, enabling cleanup of orphaned processes.
pub fn pids_dir() -> Result<PathBuf, PathError> {
    Ok(data_root()?.join("pids"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pids_dir_is_under_data_root() {
        let pids = pids_dir().expect("pids_dir failed");
        let data = data_root().expect("data_root failed");
        assert!(pids.starts_with(&data));
        assert!(pids.ends_with("pids"));
    }
}
