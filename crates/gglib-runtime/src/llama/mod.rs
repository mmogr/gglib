#![doc = include_str!("README.md")]
// === Submodules ===

pub mod args;
#[cfg(feature = "cli")]
mod build;
pub mod build_events;
mod config;
mod deps;
mod detect;
mod download;
#[cfg(feature = "cli")]
mod ensure;
pub mod error;
#[cfg(feature = "cli")]
mod install;
pub mod invocation;
pub mod progress;
pub mod prompt;
mod server_availability;
#[cfg(feature = "cli")]
mod uninstall;
#[cfg(feature = "cli")]
mod update;
mod validate;

// === Public API (facade) ===

// Error types
pub use error::{LlamaError, LlamaResult};
pub use server_availability::{LlamaServerError, LlamaServerResult, resolve_llama_server};

// Progress and prompt traits
pub use progress::{NoopProgress, ProgressReporter};
pub use prompt::{AutoConfirmPrompt, InstallPrompt, NonInteractivePrompt};

// Build pipeline event types
pub use build_events::{BuildEvent, BuildPhase};

#[cfg(feature = "cli")]
pub use deps::{check_dependencies, check_disk_space};

#[cfg(feature = "cli")]
pub use progress::CliProgress;

#[cfg(feature = "cli")]
pub use prompt::CliPrompt;

// Core functionality
pub use detect::{
    Acceleration, MissingPackage, VulkanStatus, detect_optimal_acceleration, vulkan_status,
};
pub use download::check_llama_installed;
#[cfg(feature = "cli")]
pub use ensure::ensure_llama_initialized;
pub use validate::{handle_status, validate_llama_binary};

// Installation (CLI only)
#[cfg(feature = "cli")]
pub use install::run_llama_source_build;
#[cfg(feature = "cli")]
pub use uninstall::handle_uninstall;
#[cfg(feature = "cli")]
pub use update::{handle_check_updates, handle_update};

// Command building
pub use invocation::{LlamaCommandBuilder, log_model_info};

// Args resolution
pub use args::{
    JinjaResolution,
    JinjaResolutionSource, MtpResolution, MtpResolutionSource, ReasoningDetection,
    ReasoningFormatResolution, ReasoningFormatSource, resolve_jinja_flag,
    resolve_mtp_args, resolve_reasoning_format,
};

// Prebuilt download (for adapters that need fine-grained control - Tauri + CLI)
#[cfg(feature = "prebuilt")]
pub use download::{
    LlamaProgressCallback, LlamaProgressCallbackBoxed, PrebuiltAvailability,
    check_prebuilt_availability, download_prebuilt_binaries,
    download_prebuilt_binaries_with_boxed_callback, download_prebuilt_binaries_with_callback,
};
