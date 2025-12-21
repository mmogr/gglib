#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]

pub mod assistant_ui;
mod command;
mod health;
pub mod health_monitor;
pub mod llama;
pub mod pidfile;
pub mod ports_impl;
pub mod process;
mod process_core;
pub mod proxy;
mod runner;
pub mod system;

// Re-export the main ProcessRunner implementation
pub use runner::LlamaServerRunner;

// Re-export health utilities for direct use if needed
pub use health::{check_http_health, wait_for_http_health};

// Re-export health monitoring primitives
pub use health_monitor::{ServerHealthChecker, ServerHealthMonitor};

// Re-export log sink utilities
pub use command::NoopLogSink;

// Re-export GUI process management types
pub use process::{
    CurrentModelState, GuiProcessCore, ProcessManager, ProcessStrategy, ServerEvent,
    ServerEventBroadcaster, ServerLogEntry, ServerLogManager, ServerStateInfo, ServerStatus,
    get_event_broadcaster, get_log_manager,
};

// Re-export port implementations for runtime adapters
pub use ports_impl::{CatalogPortImpl, RuntimePortImpl};

// Re-export system probe implementation
pub use system::DefaultSystemProbe;
