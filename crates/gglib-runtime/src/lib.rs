#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
// A std MutexGuard held across an .await starves the whole runtime the moment
// two tasks contend — the #721 daemon wedge was this bug class. Denied here
// because neither crate inherits the workspace clippy lints yet.
#![deny(clippy::await_holding_lock, clippy::await_holding_refcell_ref)]

pub mod assistant_ui;
mod command;
pub mod compose;
mod health;
pub mod health_monitor;
pub mod launch_narration;
pub mod llama;
pub mod pidfile;
pub mod ports_impl;
pub mod process;
pub mod proxy;
pub mod server_config;
pub mod system;
pub mod unified_server_config;

// Re-export the main ProcessRunner implementation

// Re-export health utilities for direct use if needed
pub use health::{check_http_health, wait_for_http_health};

// Re-export health monitoring primitives
pub use health_monitor::{ServerHealthChecker, ServerHealthMonitor};

// Re-export log sink utilities
pub use command::NoopLogSink;

// Re-export GUI process management types
pub use process::{
    AdmissionQueue, GuiProcessCore, PRIMARY_SLOT, ProcessManager, Resident, ResidentSet,
    SLOT_COUNT, ServerEvent, ServerEventBroadcaster, ServerLogEntry, ServerLogManager,
    ServerStateInfo, ServerStatus, get_event_broadcaster, get_log_manager,
};

// Re-export port implementations for runtime adapters
pub use ports_impl::{CatalogPortImpl, LlmCompletionAdapter, RuntimePortImpl};

// Re-export composition root factory
pub use compose::{compose_agent_loop, compose_agent_loop_with_sampling};

// Re-export system probe implementation
pub use system::DefaultSystemProbe;

// Re-export canonical ServerConfig builder for all launch surfaces
pub use server_config::{ServerConfigOptions, build_server_config};

// Re-export the unified launch config and its 3-tier cascade
pub use unified_server_config::{GlobalDefaults, UnifiedServerConfig, default_slot_dir};
