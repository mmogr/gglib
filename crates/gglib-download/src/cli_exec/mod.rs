#![doc = include_str!("README.md")]
mod api;
mod exec;
mod types;
mod utils;

pub use api::{browse_models, create_hf_api, list_quantizations, search_models};
pub use exec::{check_update, update_model};
pub use types::*;

// Re-export Python bridge for use by the async manager
pub use exec::python_bridge::{
    FastDownloadRequest, ProgressCallback, PythonBridgeError, ensure_fast_helper_ready,
    preflight_fast_helper, run_fast_download,
};
