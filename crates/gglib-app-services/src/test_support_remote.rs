//! The fixture the remote tunnel's tests are built on (ADR 0012).
//!
//! Beside `test_support.rs` rather than inside it. That file is 285 of the
//! 300 lines `scripts/check_rust_complexity.sh` allows, and a file that
//! crosses the line *joins* the baseline — the one thing the ratchet exists
//! to stop. Splitting is the house answer to a file at its budget, the same
//! answer `gglib-core`'s `settings_remote_tests.rs` is.

use std::sync::{Arc, Mutex};

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_core::services::AppCore;

use crate::test_support::test_core_and_proxy;

/// An emitter that keeps what it was told, in the order it was told.
///
/// The shape `remote/gateway_tests.rs` already uses. `RemoteOps` emits on
/// every lifecycle edge, and "emitted nothing" is as much a claim worth
/// asserting as "emitted this" — a refused `disable` that still announced
/// the tunnel was down would be a lie no return value catches.
#[derive(Default)]
pub(crate) struct RecordingEmitter(Mutex<Vec<AppEvent>>);

impl RecordingEmitter {
    /// Everything emitted so far, oldest first.
    pub(crate) fn events(&self) -> Vec<AppEvent> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl AppEventEmitter for RecordingEmitter {
    fn emit(&self, event: AppEvent) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
    }
}

/// A `RemoteOps` over its own in-memory database, plus what it emitted.
///
/// `test_core_and_proxy` returns only the core and the proxy, and
/// `RemoteOps::new` wants two more things — an emitter and a
/// `RemoteGateway` — so a `RemoteOps` could not be built in a test at all
/// before this existed.
///
/// **Nothing built here may reach `enable`.** The proxy underneath is the
/// real `ProxyOps` over a real `ProxySupervisor`, so `ensure_running` takes
/// the not-running branch and tries to bind the proxy's port for real;
/// there is no stub supervisor to hand it instead. Everything `enable` sits
/// on top of — the guards, the snapshot, the settings writes — is reachable
/// from here, and `enable` itself is what the two-machine run covers.
pub(crate) async fn test_remote_ops() -> (Arc<AppCore>, Arc<crate::RemoteOps>, Arc<RecordingEmitter>)
{
    let events = Arc::new(RecordingEmitter::default());
    // Annotated so the unsizing coercion happens here once, rather than at
    // each call that wants the trait object.
    let emitter: Arc<dyn AppEventEmitter> = events.clone();
    let (core, proxy) = test_core_and_proxy().await;
    let gateway = Arc::new(crate::RemoteGateway::new(Arc::clone(&emitter)));
    let ops = crate::RemoteOps::new(proxy, Arc::clone(&core), gateway, emitter);
    (core, Arc::new(ops), events)
}
