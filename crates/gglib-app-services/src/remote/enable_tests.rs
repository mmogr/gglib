//! Tests for [`super::RemoteOps::enable`] — what a failed enable leaves
//! behind, which has to be nothing.
//!
//! The failure is injected where modelpipe checks first and cheapest. A
//! relay value that is not a URL is refused by `validate_relay` before the
//! credential is built, before the backend is dialled and before a socket is
//! opened, so these run the whole of `enable` up to the bind — the settings
//! read, the mint, the `ServeOptions` — with no network at all and nothing
//! to tear down afterwards but the proxy.
//!
//! A real proxy is the one thing they do start: `enable` begins with
//! `ensure_running`, and the mint branch is defined by what a *running*
//! proxy demands. It is bound on loopback, so the supervisor settles on no
//! token — which is the state this whole file is about.

use std::sync::{Arc, Mutex};

use gglib_core::SettingsUpdate;

use crate::test_support_remote::test_remote_ops;
use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;

use super::*;
use crate::error::GuiError;
use crate::test_support::test_core_and_proxy;

#[derive(Default)]
pub(super) struct Recording(pub(super) Mutex<Vec<AppEvent>>);

impl AppEventEmitter for Recording {
    fn emit(&self, event: AppEvent) {
        self.0.lock().unwrap().push(event);
    }
}

/// A port nothing is listening on, so `ensure_running` binds rather than
/// colliding with whatever holds the default 8080 on this machine — a
/// developer's own daemon, most of the time.
pub(super) async fn free_port() -> u16 {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port for the test proxy");
    probe.local_addr().expect("the bound address").port()
}

/// A `RemoteOps` over the fixture, with the proxy port pointed somewhere
/// free. Nothing is running yet: `enable` starts the proxy itself, which is
/// the path being tested.
pub(super) async fn ops() -> (Arc<AppCore>, Arc<ProxyOps>, Arc<Recording>, RemoteOps) {
    let (core, proxy) = test_core_and_proxy().await;
    core.settings()
        .update(SettingsUpdate {
            proxy_port: Some(Some(free_port().await)),
            ..SettingsUpdate::default()
        })
        .await
        .expect("settings update");

    let events = Arc::new(Recording::default());
    let gateway = Arc::new(RemoteGateway::new(Arc::clone(&events) as Arc<_>));
    let ops = RemoteOps::new(
        Arc::clone(&proxy),
        Arc::clone(&core),
        gateway,
        Arc::clone(&events) as Arc<_>,
    );
    (core, proxy, events, ops)
}

/// The defect this file exists for. `enable` used to mint the API key,
/// write it to settings and sleep out the settings-cache window *before*
/// `modelpipe::serve` ran — so a tunnel that could not bind left the local
/// proxy on `127.0.0.1` demanding a bearer token from then on, with the
/// error returned to the CLI's `?` and its notice — the one thing that says
/// the door just locked, which `docs/remote.md` promises "every time it
/// runs" — never printed.
///
/// Withdrawing the key afterwards is not the repair: `disable` deliberately
/// leaves it in place (ADR 0012, decision 2) and clearing it would reopen
/// the local proxy, `/mcp` included, for anything that adopted it in the
/// meantime. Not writing it until there is a tunnel to write it for is.
#[tokio::test]
async fn an_enable_that_cannot_bring_the_tunnel_up_leaves_no_key_on_the_local_proxy() {
    let (core, proxy, _events, ops) = ops().await;

    let error = ops
        .enable(EnableRequest {
            allow_mcp: false,
            // Refused by modelpipe's first and cheapest check.
            relay: Some("not a relay url".to_owned()),
            discovery: false,
            invite: false,
        })
        .await
        .expect_err("a relay value modelpipe refuses cannot produce a tunnel");

    assert!(
        matches!(error, GuiError::Internal(ref m) if m.contains("could not start the remote tunnel")),
        "the failure is the tunnel's, and says so: {error:?}"
    );
    assert!(
        core.settings()
            .get()
            .await
            .expect("settings")
            .proxy_api_key
            .is_none(),
        "a failed enable must not leave the local proxy demanding a key nobody was told about"
    );

    proxy.stop().await.expect("the proxy the test started");
}

/// The rest of what a failed enable must not leave: no session on the
/// gateway, nothing in the `live` slot, and no `remote_enabled` on the
/// event stream for a tunnel that never came up.
#[tokio::test]
async fn a_failed_enable_arms_no_pairing_and_announces_nothing() {
    let (_core, proxy, events, ops) = ops().await;

    ops.enable(EnableRequest {
        allow_mcp: true,
        relay: Some("not a relay url".to_owned()),
        discovery: false,
        invite: false,
    })
    .await
    .expect_err("a relay value modelpipe refuses cannot produce a tunnel");

    let status = ops.status().await;
    assert!(!status.enabled, "nothing is live");
    assert!(!status.pairing_active, "and no code is redeemable");
    assert!(
        !status.mcp_allowed,
        "least of all the /mcp grant this enable asked for"
    );
    assert!(
        events.0.lock().unwrap().is_empty(),
        "nothing happened, so nothing is announced: {:?}",
        events.0.lock().unwrap()
    );

    proxy.stop().await.expect("the proxy the test started");
}

/// A second `enable` while one is live is refused before anything is
/// touched — the guard that makes the ordering above the only way state is
/// left behind.
#[tokio::test]
async fn a_failed_enable_leaves_the_next_one_free_to_run() {
    let (_core, proxy, _events, ops) = ops().await;

    for _ in 0..2 {
        let error = ops
            .enable(EnableRequest {
                allow_mcp: false,
                relay: Some("not a relay url".to_owned()),
                discovery: false,
                invite: false,
            })
            .await
            .expect_err("a relay value modelpipe refuses cannot produce a tunnel");
        assert!(
            !matches!(error, GuiError::Conflict(_)),
            "a failure must not look like an enable that is already live: {error:?}"
        );
    }

    proxy.stop().await.expect("the proxy the test started");
}

/// A machine nobody switched on does not arm itself at boot.
///
/// The daemon calls `resume` unconditionally at startup, so the switch is
/// the only thing standing between a fresh install and a tunnel nobody asked
/// for. Reaching `enable` at all here would try to bind an endpoint against
/// a relay, so a resume that returns without touching the network is exactly
/// the observation: nothing armed, nothing recorded, nothing emitted.
#[tokio::test]
async fn a_machine_that_was_never_enabled_does_not_resume() {
    let (core, ops, events) = test_remote_ops().await;

    ops.resume().await;

    assert!(
        !ops.status().await.enabled,
        "resume must not arm a tunnel on a machine that never asked for one"
    );
    assert!(
        events.events().is_empty(),
        "a resume that does nothing must say nothing"
    );
    let settings = core.settings().get().await.expect("settings readable");
    assert_eq!(
        settings.remote_enabled, None,
        "resume reads the switch; it must not write it"
    );
}

/// The switch is on but the flags are gone — the one shape `resume` refuses.
///
/// `enable` writes both together, so this is a hand-edited or half-migrated
/// database rather than anything the code produces. Arming anyway would pick
/// defaults, and the default for `--allow-mcp` is off while the machine's
/// last real answer might have been on: guessing the security posture of a
/// tunnel is worse than not arming it.
#[tokio::test]
async fn the_switch_without_the_flags_does_not_arm() {
    let (core, ops, _events) = test_remote_ops().await;
    core.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the switch is writable");

    ops.resume().await;

    assert!(
        !ops.status().await.enabled,
        "a switch with no recorded flags must not be guessed at"
    );
}

/// `disable` turns the switch off even when nothing is bound here.
///
/// The case that matters: a daemon restarted, failed to arm, and a person
/// then typed `disable`. The old code answered `Conflict` and changed
/// nothing, so the next boot would have tried to arm all over again — the
/// command would have looked like it did something and would not have.
#[tokio::test]
async fn disable_clears_the_switch_even_with_nothing_bound() {
    let (core, ops, _events) = test_remote_ops().await;
    core.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the switch is writable");

    let refused = ops.disable().await;

    assert!(
        refused.is_err(),
        "nothing was bound, so the caller still hears that"
    );
    let settings = core.settings().get().await.expect("settings readable");
    assert_eq!(
        settings.remote_enabled,
        Some(false),
        "the switch must be off regardless: the refusal is about this process, \
         the switch is about the next one"
    );
}

/// Shutting the daemon down is not a person switching remote access off.
///
/// If it were, the switch would be false after every clean exit and `resume`
/// would never fire — which is the whole feature.
#[tokio::test]
async fn shutting_down_leaves_the_switch_alone() {
    let (core, ops, _events) = test_remote_ops().await;
    core.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the switch is writable");

    let _ = ops.shut_down().await;

    let settings = core.settings().get().await.expect("settings readable");
    assert_eq!(
        settings.remote_enabled,
        Some(true),
        "a daemon stopping must leave the machine meant to be reachable"
    );
}
