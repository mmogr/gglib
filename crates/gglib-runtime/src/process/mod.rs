#![doc = include_str!("README.md")]
mod broadcaster;
mod core;
mod events;
mod health;
mod logs;
mod manager;
mod ports;
pub mod shutdown;
mod stream;
mod types;

// Re-export commonly used types
pub use broadcaster::{ServerEventBroadcaster, get_event_broadcaster};
pub use core::GuiProcessCore;
pub use events::{ServerEvent, ServerStateInfo, ServerStatus};
pub use health::{check_http_health, check_process_health, update_health_batch, wait_for_http_health};
pub use logs::{LogManagerSink, ServerLogEntry, ServerLogManager, get_log_manager};
pub use manager::{CurrentModelState, ProcessManager, ProcessStrategy};
pub use shutdown::{kill_pid, shutdown_child};
pub(crate) use stream::spawn_stream_reader;
pub use types::{RunningProcess, ServerInfo};
