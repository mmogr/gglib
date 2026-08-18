#![doc = include_str!("README.md")]
mod api;
mod exec;
mod types;
mod utils;

pub use api::{create_hf_api, list_quantizations};
pub use exec::{check_update, update_model};
pub use types::*;

// Re-export Python bridge for use by the async manager
pub use exec::python_bridge::{
    FastDownloadRequest, NoticeCallback, ProgressCallback, PythonBridgeError,
    ensure_fast_helper_ready, ensure_fast_helper_ready_with_python, preflight_fast_helper,
    run_fast_download,
};

// Whether the optional hf_xet accelerator is already provisioned. Read by the
// executor to choose a backend, and by the GUI to render setup status.
pub use exec::python_env::fast_helper_provisioned;

// Inspecting and removing that environment, for `gglib config fast-downloads`.
// Neither is on the download path: the backend choice stays the bare
// file-existence check above.
pub use exec::python_env::{FastHelperStatus, fast_helper_status, remove_fast_helper};
