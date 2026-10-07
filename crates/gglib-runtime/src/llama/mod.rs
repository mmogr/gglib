#![doc = include_str!("README.md")]
// === Submodules ===

pub mod args;
#[allow(
    clippy::option_if_let_else,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod build;
pub mod build_events;
mod config;
mod deps;
mod detect;
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::items_after_statements,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::needless_collect,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod download;
mod ensure;
pub mod error;
mod install;
pub mod install_events;
pub mod prompt;
pub mod runtime_probe;
mod server_availability;
mod status;
mod uninstall;
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

// Prompt trait and its three policies
pub use prompt::{AutoConfirmPrompt, CliPrompt, InstallPrompt, NonInteractivePrompt};

// Build pipeline event types
pub use build_events::{BuildEvent, BuildPhase};

// Prebuilt install pipeline event types
pub use install_events::{InstallPhase, LlamaProgressEvent};

pub use deps::check_dependencies;

// Core functionality
pub use detect::{
    Acceleration, MissingPackage, VulkanStatus, detect_optimal_acceleration, vulkan_status,
};
pub use download::check_llama_installed;
pub use ensure::ensure_llama_initialized;
pub use status::{LlamaBuildInfo, LlamaStatus, llama_status};
pub use validate::{handle_status, validate_llama_binary};

// Installation
pub use install::run_llama_source_build;
pub use uninstall::{UninstallOutcome, handle_uninstall, uninstall_llama};
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

// Prebuilt download (for adapters that need fine-grained control: the daemon and the CLI)
pub use download::{PrebuiltAvailability, check_prebuilt_availability, download_prebuilt_binaries};
