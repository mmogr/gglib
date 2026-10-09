//! The startup orphan sweep, run for real against a pidfile directory (#955).
//!
//! The only test in this binary, on purpose. `cleanup_orphaned_servers`
//! deletes every pidfile whose process it cannot verify and kills every one
//! it can, which is why outside tests it runs under the daemon lock. Here the
//! binary stands in for the lock: its data root is its own
//! (`isolate_data_root`), and nothing else writes a pidfile into it. In the
//! crate's unit-test binary it would remove the pidfiles of the children
//! `launch_tests` and `core_drop_tests` spawn while they still read them.
//!
//! Nothing it finds can be killed. The resource root is the test root too,
//! where neither llama-server nor `sd-server` is installed, so `is_our_server`
//! verifies no process and the sweep only removes files.

use gglib_core::paths::{isolate_data_root, llama_server_path, pids_dir, sd_server_path};
use gglib_runtime::pidfile::{cleanup_orphaned_servers, list_pidfiles, write_pidfile};

#[tokio::test]
async fn cleanup_removes_stale_pidfiles() {
    // Checked before anything is written or swept: a sweep that ran with
    // either outside this root would be the #955 incident, and pass.
    let root = isolate_data_root();
    assert!(pids_dir().unwrap().starts_with(root));
    assert!(llama_server_path().unwrap().starts_with(root));
    assert!(sd_server_path().unwrap().starts_with(root));

    write_pidfile(1, 999_999, 9999).expect("write failed");

    cleanup_orphaned_servers().await.expect("cleanup failed");

    let left = list_pidfiles().expect("list failed");
    assert!(left.is_empty(), "the sweep left {left:?}");
}
