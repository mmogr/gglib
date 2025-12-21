//! PID file management for tracking llama-server processes.
//!
//! Provides atomic I/O, process verification, and startup orphan cleanup.
//!
//! # Safety guarantees
//! - Atomic writes via temp file + rename
//! - Process verification before killing (prevents PID reuse issues)
//! - Conservative cleanup (if verification fails, only delete PID file)

mod io;
mod sweep;
mod verify;

pub use io::{PidFileData, delete_pidfile, list_pidfiles, read_pidfile, write_pidfile};
pub use sweep::cleanup_orphaned_servers;
pub use verify::{is_our_llama_server, pid_exists};
