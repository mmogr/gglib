#![doc = include_str!("README.md")]

mod cell;
mod chat;
mod door;
mod executor;
mod reader;
mod registry;
mod sse;

pub use registry::RunRegistry;

use std::sync::Arc;

use gglib_core::services::AppCore;

use crate::proxy::ProxyOps;

/// Milliseconds since the Unix epoch. An argument, so tests drive time.
pub(crate) type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The wall clock.
fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    })
}

/// The daemon's registry: chat runs through its own proxy, on the wall clock.
pub(crate) fn chat_registry(proxy: Arc<ProxyOps>, core: Arc<AppCore>) -> RunRegistry {
    let door = Arc::new(door::LocalProxy { proxy, core });
    let executor = Arc::new(chat::ChatExecutor::new(door));
    RunRegistry::new(executor, system_clock())
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod chat_tests;
#[cfg(test)]
#[path = "door_tests.rs"]
mod door_tests;
#[cfg(test)]
#[path = "event_limit_tests.rs"]
mod event_limit_tests;
#[cfg(test)]
mod fake_proxy;
#[cfg(test)]
#[path = "limits_tests.rs"]
mod limits_tests;
#[cfg(test)]
#[path = "privacy_tests.rs"]
mod privacy_tests;
#[cfg(test)]
#[path = "redaction_tests.rs"]
mod redaction_tests;
#[cfg(test)]
#[path = "registry_tests.rs"]
mod registry_tests;
#[cfg(test)]
#[path = "retention_tests.rs"]
mod retention_tests;
#[cfg(test)]
#[path = "scope_tests.rs"]
mod scope_tests;
#[cfg(test)]
pub(crate) mod test_executor;
