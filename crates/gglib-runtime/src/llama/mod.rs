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
pub(crate) mod detect;
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    clippy::items_after_statements,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::needless_collect,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod download;
mod install;
pub mod install_events;
pub mod runtime_probe;
mod server_availability;
mod status;
mod uninstall;
mod update;
mod update_preflight;
mod validate;

// === Public API (facade) ===

// The build an update replaces, as its record names it, and the one question
// a launch asks of that record: which acceleration the binary it is about to
// spawn was compiled for.
pub use config::BuildConfig;
pub(crate) use config::recorded_acceleration;
pub use server_availability::{LlamaServerError, LlamaServerResult, resolve_llama_server};

// What the installed llama-server can do natively. Probed once per binary and
// held for the run — see `runtime_probe` for why arbitration is static.
pub use runtime_probe::probe as probe_runtime_capabilities;

// Build pipeline event types
pub use build_events::{BuildEvent, BuildPhase};

// Prebuilt install pipeline event types
pub use install_events::{InstallPhase, LlamaProgressEvent};

// The tools a source build runs, and how to install them
pub use deps::{BuildTool, build_tool_install_lines, build_tools, missing_build_tools};

// Core functionality
pub use detect::{
    Acceleration, MissingPackage, VulkanStatus, detect_optimal_acceleration, vulkan_status,
};
pub use download::check_llama_installed;
pub use status::{LlamaBuildInfo, LlamaPrebuiltInfo, LlamaStatus, llama_status};
pub use validate::{handle_status, validate_llama_binary};

// Installation
pub use install::run_llama_source_build;
pub use uninstall::{UninstallOutcome, llama_files_present, uninstall_llama};
pub use update::{LlamaUpdateCheck, handle_check_updates, llama_update_check, run_llama_update};
pub use update_preflight::{LOCAL_CHANGES_CAUTION, UpdatePlan, UpdateRefusal, update_preflight};

// Args resolution
pub use args::{
    JinjaResolution, JinjaResolutionSource, MtpResolution, MtpResolutionSource, ReasoningDetection,
    ReasoningFormatResolution, ReasoningFormatSource, resolve_jinja_flag, resolve_mtp_args,
    resolve_reasoning_format,
};

// Prebuilt download (for adapters that need fine-grained control: the daemon and the CLI)
pub use download::{PrebuiltAvailability, check_prebuilt_availability, download_prebuilt_binaries};
