//! The fixture the remote tunnel's tests are built on (ADR 0012).
//!
//! Beside `test_support.rs` rather than inside it. That file is 285 of the
//! 300 lines `scripts/check_rust_complexity.sh` allows, and a file that
//! crosses the line *joins* the baseline — the one thing the ratchet exists
//! to stop. Splitting is the house answer to a file at its budget, the same
//! answer `gglib-core`'s `settings_remote_tests.rs` is.
//!
//! It also carries the machines the tests name, because `connect_tests.rs`
//! and `lifecycle_tests.rs` name the same ones and a second copy of a
//! ticket is a second thing to keep true. They are modelpipe's own normative
//! vectors from `docs/ticket-format-v0.md`, which ship in its published
//! tarball and are asserted identical by three implementations on every one
//! of its CI runs.
//! `ticket_vectors.py` has no `--update` flag, deliberately, so these
//! strings cannot drift under us.

use std::sync::{Arc, Mutex};

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_core::services::AppCore;
use gglib_core::{RemotePairing, SettingsUpdate};

use crate::test_support::test_core_and_proxy;

/// Vector 1: the minimal v0 ticket, no transport addresses.
pub(crate) const TICKET_A: &str =
    "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na";

/// The first six bytes of vector 1's endpoint id, which is what a
/// fingerprint shows.
pub(crate) const FINGERPRINT_A: &str = "d75a980182b1";

/// A *second* machine: the same minimal shape as vector 1 over the public
/// key from RFC 8032 §7.1 TEST 2, so the two tickets name genuinely
/// different endpoints rather than one endpoint at two addresses. Every
/// published vector shares TEST 1's key, so no pair of them could say this.
pub(crate) const TICKET_B: &str =
    "pipeaa6uaf6d5bbyswusw4fkoti3p26jzgbmz4xmjfumydgvl4jk6rtayaaa2e4g6hq";

/// Vector 1's key with vector 3's address set: one IPv6 address in the
/// documentation prefix (RFC 3849), which routes nowhere anywhere. The only
/// ticket here that is ever dialled.
pub(crate) const TICKET_UNREACHABLE: &str = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaicaajcaainxaaaaaaaaaaaaaaaaaaach4qaabstehw";

/// The same string again, under the name the pairing record cares about:
/// vector 1's endpoint key at an address vector 1 does not carry, which is
/// machine A having moved. Two names for one constant rather than two
/// constants, so nothing can drift between them — and a second name because
/// "unreachable" is the wrong word entirely where the claim is that a
/// codeless dial carries the key across an address change.
pub(crate) const TICKET_A_MOVED: &str = TICKET_UNREACHABLE;

pub(crate) const KEY_A: &str = "sk-zzq-the-key-machine-a-handed-over";
pub(crate) const KEY_B: &str = "sk-zzq-the-key-machine-b-handed-over";

/// The pairing `connect` writes once a code has been redeemed, as a
/// settings update.
pub(crate) fn paired_with(ticket: &str, api_key: &str) -> SettingsUpdate {
    SettingsUpdate {
        remote_pairing: Some(Some(RemotePairing {
            ticket: ticket.to_owned(),
            api_key: api_key.to_owned(),
            default_model: None,
            port: None,
        })),
        ..SettingsUpdate::default()
    }
}

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

/// One of the vectors above as the `Ticket` the code under test takes.
///
/// Here rather than in either test module because both halves of `settle`
/// are tested in their own file now, and a second `parse().expect()` is a
/// second place for the panic message to be wrong.
pub(crate) fn ticket(s: &str) -> modelpipe::Ticket {
    s.parse().expect("a normative ticket vector parses")
}

/// The redemption a codeless dial must not reach.
///
/// A closure that cannot be called is a stronger claim than one that
/// records that it was not.
pub(crate) async fn never_redeems(_code: String) -> Result<String, crate::GuiError> {
    unreachable!("a dial with no code has nothing to redeem")
}
