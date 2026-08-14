#![doc = include_str!("README.md")]
// `pub(crate)` for the launch narration: the banner is the proxy's output
// voice, but the launch it narrates happens in `process::swap_state`.
pub(crate) mod banner;
pub mod supervisor;

// Re-export supervisor types
pub use supervisor::{ProxyBind, ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};
// Re-exported so ProxyConfig consumers (the daemon's start handler) can fill
// `disk_budget` without a direct gglib-proxy dependency.
pub use gglib_proxy::slot_eviction::{DiskBudget, resolve_disk_budget};
