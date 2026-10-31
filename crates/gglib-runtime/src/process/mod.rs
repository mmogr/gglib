#![doc = include_str!("README.md")]
pub mod admission;
mod broadcaster;
mod core;
mod events;
mod health;
mod logs;
mod manager;
mod ports;
#[allow(
    clippy::significant_drop_tightening,
    reason = "a guard's scope is its critical section, so this lint is never applied \
              in admission, residency or proxy lock code"
)]
pub mod residency;
pub mod shutdown;
mod spawn_config;
mod stream;
mod types;

// Re-export commonly used types
pub use admission::{AdmissionQueue, PRIMARY_SLOT, RENDER_STOPPED, Resident, SLOT_COUNT};
pub use broadcaster::{ServerEventBroadcaster, get_event_broadcaster};
pub use core::GuiProcessCore;
pub use events::{ServerEvent, ServerStateInfo, ServerStatus};
pub use health::{check_http_health, wait_for_http_health};
pub use logs::{ServerLogEntry, ServerLogManager, get_log_manager};
pub use manager::ProcessManager;
pub use residency::ResidentSet;
pub use shutdown::{kill_pid, shutdown_child};
pub use spawn_config::{RuntimeBinaries, SpawnConfig};
pub(crate) use stream::spawn_stream_reader;
pub use types::{RunningProcess, ServerInfo};
