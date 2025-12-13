//! Llama.cpp management for gglib-runtime.
//!
//! This module provides all llama.cpp-related functionality:
//! - Installation (pre-built download or source build)
//! - Hardware acceleration detection (Metal, CUDA, CPU)
//! - Binary validation and status checking
//! - Update management
//! - Command invocation building
//!
//! # Public API
//!
//! The public API is intentionally minimal. Import from `gglib_runtime::llama`:
//!
//! ```rust,ignore
//! use gglib_runtime::llama::{
//!     ensure_llama_initialized,
//!     check_llama_installed,
//!     LlamaCommandBuilder,
//!     resolve_context_size,
//! };
//! ```
//!
//! # Feature Flags
//!
//! - `cli`: Enables `CliProgress` and `CliPrompt` for interactive CLI usage.

// === Submodules ===

pub mod args;
mod build;
mod config;
mod deps;
mod detect;
mod download;
mod ensure;
pub mod error;
mod install;
pub mod invocation;
pub mod progress;
pub mod prompt;
mod server_availability;
mod uninstall;
mod update;
mod validate;

// === Public API (facade) ===

// Error types
pub use error::{LlamaError, LlamaResult};
pub use server_availability::{LlamaServerError, LlamaServerResult, resolve_llama_server};

// Progress and prompt traits
pub use progress::{NoopProgress, ProgressReporter};
pub use prompt::{AutoConfirmPrompt, InstallPrompt, NonInteractivePrompt};

#[cfg(feature = "cli")]
pub use progress::CliProgress;

#[cfg(feature = "cli")]
pub use prompt::CliPrompt;

// Core functionality
pub use detect::{Acceleration, detect_optimal_acceleration};
pub use download::check_llama_installed;
pub use ensure::ensure_llama_initialized;
pub use validate::{handle_status, validate_llama_binary, validate_llama_cli_binary};

// Installation
pub use install::handle_install;
pub use uninstall::{handle_rebuild, handle_uninstall};
pub use update::{handle_check_updates, handle_update};

// Command building
pub use invocation::{LlamaCommandBuilder, log_context_info, log_model_info};

// Args resolution
pub use args::{
    ContextResolution, ContextResolutionSource, JinjaResolution, JinjaResolutionSource,
    ReasoningDetection, ReasoningFormatResolution, ReasoningFormatSource, resolve_context_size,
    resolve_jinja_flag, resolve_reasoning_format,
};

// Download (for adapters that need fine-grained control)
pub use download::{
    LlamaProgressCallback, LlamaProgressCallbackBoxed, PrebuiltAvailability,
    check_prebuilt_availability, download_prebuilt_binaries,
    download_prebuilt_binaries_with_boxed_callback, download_prebuilt_binaries_with_callback,
};
