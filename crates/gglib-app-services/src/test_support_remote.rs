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

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

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

/// The pairing `join` writes once a code has been redeemed, as a
/// settings update.
pub(crate) fn paired_with(ticket: &str, api_key: &str) -> SettingsUpdate {
    SettingsUpdate {
        remote_pairing: Some(Some(RemotePairing {
            ticket: ticket.to_owned(),
            api_key: api_key.to_owned(),
            default_model: None,
            port: None,
            name: None,
        })),
        ..SettingsUpdate::default()
    }
}

/// Remember `model` on the stored pairing, as a `--remote` turn does: that
/// field of the record as it stands, and nothing else.
pub(crate) async fn remember_a_model(core: &AppCore, model: &str) {
    core.settings()
        .repo()
        .modify(&|settings: &mut gglib_core::Settings| {
            if let Some(pairing) = settings.remote_pairing.as_mut() {
                pairing.default_model = Some(model.to_owned());
            }
            Ok(())
        })
        .await
        .expect("the model is remembered");
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
///
/// Its join keys are kept in a directory of its own, for the reason its
/// device keys are, and one more: two tests that dial one machine at once
/// could both mint its key into one file, and the one that lost the race
/// would have its join refused.
pub(crate) async fn test_remote_ops() -> (Arc<AppCore>, Arc<crate::RemoteOps>, Arc<RecordingEmitter>)
{
    test_remote_ops_joining_from(scratch_join_keys()).await
}

/// [`test_remote_ops`] with its join keys kept in `join_keys`, so two of
/// them can be one machine before and after a restart.
pub(crate) async fn test_remote_ops_joining_from(
    join_keys: PathBuf,
) -> (Arc<AppCore>, Arc<crate::RemoteOps>, Arc<RecordingEmitter>) {
    let events = Arc::new(RecordingEmitter::default());
    // Annotated so the unsizing coercion happens here once, rather than at
    // each call that wants the trait object.
    let emitter: Arc<dyn AppEventEmitter> = events.clone();
    let (core, proxy) = test_core_and_proxy().await;
    let gateway = Arc::new(crate::RemoteGateway::new(Arc::clone(&emitter)));
    let ops = crate::RemoteOps::new(
        proxy,
        Arc::clone(&core),
        gateway,
        emitter,
        Some(scratch_device_keys()),
    )
    .with_join_keys(join_keys);
    (core, Arc::new(ops), events)
}

/// A device key file no other `RemoteOps` in this run reads or writes.
///
/// Given `None`, `RemoteOps` keeps keys beside the endpoint identity, in
/// `data/remote_devices` under this binary's own data root (#955): one file
/// for every test in the process.
pub(crate) fn scratch_device_keys() -> PathBuf {
    scratch("remote_devices")
}

/// A directory for the keys a `RemoteOps` joins with that no other in this
/// run uses, for [`RemoteOps::with_join_keys`](crate::RemoteOps::with_join_keys).
///
/// Without one, `RemoteOps` keeps them in `data/remote_join` under this
/// binary's own data root: one directory for every test in the process.
/// Named and not made: a dial makes it, and whether it does is under test.
pub(crate) fn scratch_join_keys() -> PathBuf {
    scratch("remote_join")
}

/// A new path starting `name` in a directory keyed by the process id and
/// emptied once per process, because a recycled pid can find files a dead
/// run left there.
fn scratch(name: &str) -> PathBuf {
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        let dir = std::env::temp_dir().join(format!("gglib-app-services-{}", std::process::id()));
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::NotFound,
                "could not empty the scratch directory {dir:?}: {e}"
            );
        }
        dir
    });
    static NEXT: AtomicU64 = AtomicU64::new(0);
    DIR.join(format!("{name}-{}", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// One of the vectors above as the `Ticket` the code under test takes.
///
/// Here rather than in either test module because both halves of `settle`
/// are tested in their own file, and a second `parse().expect()` is a
/// second place for the panic message to be wrong.
pub(crate) fn ticket(s: &str) -> modelpipe::Ticket {
    s.parse().expect("a normative ticket vector parses")
}

/// What `probe` finds, asked again until it finds something, or `None` after
/// two seconds.
///
/// For what a task or the far end of a pipe writes, which leaves no handle to
/// await. A deadline says how long "a moment" is allowed to be, and `None`
/// rather than a panic lets the caller clean up before it judges.
pub(crate) async fn within_a_moment<T>(mut probe: impl AsyncFnMut() -> Option<T>) -> Option<T> {
    for _ in 0..100 {
        if let Some(found) = probe().await {
            return Some(found);
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    None
}

/// The redemption a codeless dial must not reach.
///
/// A closure that cannot be called is a stronger claim than one that
/// records that it was not.
pub(crate) async fn never_redeems(_code: String) -> Result<String, crate::GuiError> {
    unreachable!("a dial with no code has nothing to redeem")
}
