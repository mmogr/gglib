//! CLI download execution layer.
//!
//! This module provides the download execution logic for CLI commands.
//! It is intentionally separated from the async queue-based `DownloadManagerPort`
//! which is designed for GUI/background downloads.
//!
//! # Architecture
//!
//! The CLI execution layer:
//! - Uses synchronous/blocking patterns suitable for CLI UX
//! - Shows progress bars directly in terminal
//! - Returns results for the handler to register in the database
//!
//! # Usage
//!
//! ```ignore
//! use gglib_download::cli_exec::{CliDownloadRequest, download};
//!
//! let request = CliDownloadRequest::new("unsloth/Llama-3-GGUF", models_dir)
//!     .with_quantization("Q4_K_M");
//!
//! let result = download(request).await?;
//! // Handler then calls ctx.app().models().add(...) with result
//! ```
//!
//! # No `AppCore`
//!
//! This module does NOT import or use `AppCore`. Database registration
//! is the responsibility of the CLI handler layer.

mod api;
mod exec;
mod types;
mod utils;

pub use api::{browse_models, create_hf_api, list_quantizations, search_models};
pub use exec::{check_update, download, update_model};
pub use types::*;

// Re-export Python bridge for use by the async manager
pub use exec::python_bridge::{
    FastDownloadRequest, PythonBridgeError, ensure_fast_helper_ready, run_fast_download,
};
