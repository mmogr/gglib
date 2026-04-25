//! CLI download utility layer.
//!
//! This module provides utilities used by CLI commands that are intentionally
//! separated from the queue-based [`DownloadManagerPort`] path.
//!
//! # What lives here
//!
//! - [`list_quantizations`] — `HuggingFace` quant listing for `--list-quants`
//! - [`check_update`] / [`update_model`] — update path for `model upgrade`
//! - Python bridge helpers ([`ensure_fast_helper_ready`], [`run_fast_download`]) shared
//!   with the async download manager
//!
//! # What moved out
//!
//! Interactive downloads (the `model download` command) now route through
//! [`DownloadManagerPort::queue_smart`], giving the CLI the same queue,
//! progress events, and model registration path as the GUI.

mod api;
mod exec;
mod types;
mod utils;

pub use api::{browse_models, create_hf_api, list_quantizations, search_models};
pub use exec::{check_update, update_model};
pub use types::*;

// Re-export Python bridge for use by the async manager
pub use exec::python_bridge::{
    FastDownloadRequest, PythonBridgeError, ensure_fast_helper_ready, preflight_fast_helper,
    run_fast_download,
};
