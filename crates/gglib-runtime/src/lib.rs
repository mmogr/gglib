#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
// A std MutexGuard held across an .await starves the whole runtime the moment
// two tasks contend (the bug class of #721). The workspace lints only warn on
// it; here it is an error wherever clippy runs.
#![deny(clippy::await_holding_lock, clippy::await_holding_refcell_ref)]

pub(crate) mod binary_install;
mod command;
// Crate-internal: no consumer names these four by path — `compose`,
// `health_monitor` and `server_config` are reached through the re-exports
// below, and nothing outside this crate touches `launch_narration` at all.
pub(crate) mod compose;
mod health;
pub(crate) mod health_monitor;
#[allow(
    clippy::many_single_char_names,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod launch_narration;
pub mod llama;
pub mod pidfile;
pub mod ports_impl;
pub mod process;
pub mod proxy;
pub(crate) mod server_config;
#[allow(
    clippy::redundant_closure_for_method_calls,
    clippy::too_many_lines,
    clippy::used_underscore_binding,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod system;
pub mod unified_server_config;

// Re-export health monitoring primitives
pub use health_monitor::{ServerHealthChecker, ServerHealthMonitor};

// Re-export GUI process management types
pub use process::{
    AdmissionQueue, GuiProcessCore, PRIMARY_SLOT, ProcessManager, Resident, ResidentSet,
    SLOT_COUNT, ServerEvent, ServerEventBroadcaster, ServerLogEntry, ServerLogManager,
    ServerStateInfo, ServerStatus, get_event_broadcaster, get_log_manager,
};

// Re-export port implementations for runtime adapters
pub use ports_impl::{
    CatalogPortImpl, FarMachine, LlmCompletionAdapter, RuntimePortImpl, SamplingObserver,
};

// Re-export composition root factory
pub use compose::{compose_agent_loop, compose_agent_loop_with_sampling};

// Re-export system probe implementation
pub use system::DefaultSystemProbe;

// Re-export canonical ServerConfig builder for all launch surfaces
pub use server_config::{ServerConfigOptions, build_server_config};

// Re-export the unified launch config and its 3-tier cascade
pub use unified_server_config::{GlobalDefaults, UnifiedServerConfig, default_slot_dir};
