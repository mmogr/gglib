//! Process management infrastructure for GUI applications.
//!
//! This module provides shared infrastructure for managing llama-server processes
//! with integrated log streaming and event broadcasting for GUI use cases.
//!
//! # Structure
//!
//! - `GuiProcessCore` - Low-level process spawning with log streaming (u32 model IDs)
//! - `ProcessManager` - High-level concurrent process orchestration
//! - `ServerEvent` / `ServerEventBroadcaster` - Lifecycle event broadcasting
//! - `ServerLogManager` - Log streaming infrastructure
//! - Health check utilities
//!
//! # Distinction from `ProcessCore`
//!
//! This module's `GuiProcessCore` is distinct from the port-aligned `ProcessCore`
//! in `process_core.rs`. The port version implements `ProcessRunner` for CLI use
//! with `i64` model IDs and no log/event infrastructure.

mod broadcaster;
mod core;
mod events;
mod health;
mod logs;
mod stream;
mod manager;
mod ports;
pub mod shutdown;
mod types;

// Re-export commonly used types
pub use broadcaster::{ServerEventBroadcaster, get_event_broadcaster};
pub use core::GuiProcessCore;
pub use events::{ServerEvent, ServerStateInfo, ServerStatus};
pub use health::{check_process_health, update_health_batch, wait_for_http_health};
pub use logs::{LogManagerSink, ServerLogEntry, ServerLogManager, get_log_manager};
pub(crate) use stream::spawn_stream_reader;
pub use manager::{CurrentModelState, ProcessManager, ProcessStrategy};
pub use shutdown::{kill_pid, shutdown_child};
pub use types::{RunningProcess, ServerInfo};
