#![doc = include_str!("README.md")]
// `pub(crate)` for the launch narration: the banner is the proxy's output
// voice, but the launch it narrates happens in `process::swap_state`.
mod api_key;
pub(crate) mod banner;
#[allow(
    clippy::significant_drop_tightening,
    reason = "a guard's scope is its critical section, so this lint is never applied \
              in admission, residency or proxy lock code"
)]
#[allow(
    clippy::manual_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub mod supervisor;

// Re-export supervisor types
pub use supervisor::{ProxyBind, ProxyConfig, ProxyStatus, ProxySupervisor, SupervisorError};
// Re-exported so ProxyConfig consumers (the daemon's start handler) can fill
// `disk_budget` without a direct gglib-proxy dependency.
pub use gglib_proxy::slot_eviction::{DiskBudget, resolve_disk_budget};
