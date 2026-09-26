#![doc = include_str!("README.md")]
pub mod admission;
mod broadcaster;
#[allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::unused_async,
    clippy::unused_self,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod core;
mod events;
mod health;
mod logs;
#[allow(
    clippy::doc_link_code,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod manager;
mod ports;
#[allow(
    clippy::significant_drop_tightening,
    reason = "a guard's scope is its critical section, so this lint is never applied \
              in admission, residency or proxy lock code"
)]
#[allow(
    clippy::option_if_let_else,
    clippy::single_match_else,
    clippy::unused_self,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod residency;
pub mod shutdown;
mod stream;
mod types;

// Re-export commonly used types
pub use admission::{AdmissionQueue, PRIMARY_SLOT, Resident, SLOT_COUNT};
pub use broadcaster::{ServerEventBroadcaster, get_event_broadcaster};
pub use core::GuiProcessCore;
pub use events::{ServerEvent, ServerStateInfo, ServerStatus};
pub use health::{check_http_health, wait_for_http_health};
pub use logs::{LogManagerSink, ServerLogEntry, ServerLogManager, get_log_manager};
pub use manager::ProcessManager;
pub use residency::ResidentSet;
pub use shutdown::{kill_pid, shutdown_child};
pub(crate) use stream::spawn_stream_reader;
pub use types::{RunningProcess, ServerInfo};
