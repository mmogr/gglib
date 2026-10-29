//! The library every test of the exports is built over, and how a test
//! waits on an export.
//!
//! A test drives the exports as the app does, from a thread with no tokio
//! context, by blocking on the library's own runtime.

use std::future::Future;
use std::sync::Arc;

use gglib_bootstrap::CoreBootstrap;
use gglib_core::ports::HfClientPort;
use tempfile::TempDir;

use crate::download_feed::DownloadFeed;
use crate::library::{Library, LibraryDirs, config_for};
use crate::reading::DeviceReading;
use crate::runtime::runtime;
use crate::test_hub::TestHub;

const GIB: u64 = 1 << 30;

/// A phone with 8 GiB of memory and 6 GiB of it free.
pub(crate) const PHONE: DeviceReading = DeviceReading {
    physical_bytes: 8 * GIB,
    available_bytes: 6 * GIB,
    is_simulator: false,
};

/// Run `call` to its end, as the app's thread waits on it.
pub(crate) fn wait<F: Future>(call: F) -> F::Output {
    runtime().block_on(call)
}

/// The folders of a library under `root`, in a process whose gglib data
/// root is its own: what a library resolves for itself, such as where the
/// download accelerator's environment would be, is never the user's.
pub(crate) fn dirs_in(root: &TempDir) -> LibraryDirs {
    gglib_core::paths::isolate_data_root();
    LibraryDirs {
        data_dir: root.path().join("data").to_string_lossy().into_owned(),
        models_dir: root.path().join("models").to_string_lossy().into_owned(),
    }
}

/// A library in `root`'s folders over `hub`, built by the composition root
/// as [`crate::open_library`] builds one.
pub(crate) fn library_over(hub: TestHub, root: &TempDir) -> Arc<Library> {
    let hub: Arc<dyn HfClientPort> = Arc::new(hub);
    wait(async {
        let (config, models_dir) = config_for(&dirs_in(root), None).expect("the folders");
        let feed = Arc::new(DownloadFeed::new());
        let core = CoreBootstrap::build_with_hub(config, Arc::clone(&feed) as _, hub)
            .await
            .expect("a library over a stand-in Hub");
        Arc::new(Library::over(core, feed, models_dir))
    })
}
