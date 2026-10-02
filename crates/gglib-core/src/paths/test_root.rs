//! A data root of a test binary's own.
//!
//! A debug build resolves its data root into the checkout it was built from
//! (`platform::detect_local_repo`), and an installed `gglib` built from that
//! checkout keeps its database, pidfiles, daemon lock and logs in the same
//! place. A test that resolves a path through [`super::data_root`] would
//! share all of them with a running daemon (#955).
//!
//! [`isolate_data_root`] gives the calling process a temporary directory
//! instead, which [`super::data_root`] and [`super::resource_root`] answer
//! before anything else, `GGLIB_DATA_DIR` and `GGLIB_RESOURCE_DIR` included.
//! It is set once and stays for the life of the process: tests in one binary
//! share it, and only tests in different binaries are apart.
//!
//! Compiled only with this crate's `test-utils` feature, which other crates
//! enable from `[dev-dependencies]`. Without it this module does not exist,
//! nothing can set a root, and the resolvers do not look for one.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// The root [`isolate_data_root`] set, if a test in this process called it.
pub(super) fn root() -> Option<&'static Path> {
    ROOT.get().map(PathBuf::as_path)
}

/// Give this process a data root of its own, and return it.
///
/// The first call creates an empty directory under the system's temporary
/// directory; every later call returns that one. Call it at the start of any
/// test that reaches the data or resource root, directly or through
/// `pids_dir`, `database_path`, `llama_server_path` or the log directory,
/// because a path resolved before the first call is the checkout's.
///
/// The directory is not removed when the process exits: a value held in a
/// `static` is never dropped. It holds only what the tests wrote there.
///
/// # Panics
///
/// If the temporary directory cannot be created.
pub fn isolate_data_root() -> &'static Path {
    ROOT.get_or_init(|| {
        tempfile::Builder::new()
            .prefix("gglib-test-root-")
            .tempdir()
            .expect("create a data root for this test process")
            .keep()
    })
}
