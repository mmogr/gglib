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
pub mod progress;
pub mod prompt;
pub mod runtime_probe;
mod server_availability;
mod status;
#[cfg(feature = "cli")]
mod uninstall;
#[cfg(feature = "cli")]
mod update;
mod validate;

// === Public API (facade) ===

// Error types
pub use error::{LlamaError, LlamaResult};

// The installed build's recorded configuration. Exposed so a launch can name
// which acceleration the binary it is about to spawn was compiled for.
pub use config::BuildConfig;
pub use server_availability::{LlamaServerError, LlamaServerResult, resolve_llama_server};

// What the installed llama-server can do natively. Probed once per binary and
// held for the run — see `runtime_probe` for why arbitration is static.
pub use runtime_probe::probe as probe_runtime_capabilities;

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
pub use status::{LlamaBuildInfo, LlamaStatus, llama_status};
pub use validate::{handle_status, validate_llama_binary};

// Installation (CLI only)
#[cfg(feature = "cli")]
pub use install::run_llama_source_build;
#[cfg(feature = "cli")]
pub use uninstall::{UninstallOutcome, handle_uninstall, uninstall_llama};
#[cfg(feature = "cli")]
pub use update::{
    LlamaUpdateCheck, handle_check_updates, handle_update, llama_update_check, run_llama_update,
    update_acceleration,
};

// Args resolution
pub use args::{
    JinjaResolution, JinjaResolutionSource, MtpResolution, MtpResolutionSource, ReasoningDetection,
    ReasoningFormatResolution, ReasoningFormatSource, resolve_jinja_flag, resolve_mtp_args,
    resolve_reasoning_format,
};

// Prebuilt download (for adapters that need fine-grained control - Tauri + CLI)
#[cfg(feature = "prebuilt")]
pub use download::{
    LlamaProgressCallback, LlamaProgressCallbackBoxed, PrebuiltAvailability,
    check_prebuilt_availability, download_prebuilt_binaries,
    download_prebuilt_binaries_with_boxed_callback, download_prebuilt_binaries_with_callback,
};
