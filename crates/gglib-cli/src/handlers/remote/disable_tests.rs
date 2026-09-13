//! Tests for what `gglib remote disable` prints, and for the switch it clears
//! with no daemon running.
//!
//! A `#[path]` sibling of `disable.rs` rather than an inline `mod tests`,
//! because the two together would cross the 300-line budget — the same split
//! `wire_tests.rs` and `serve_switch.rs` were made along.

use gglib_core::SettingsUpdate;
use gglib_core::services::AppCore;

use super::*;

/// [#1005]: the notice may keep the half that is always true and must not
/// keep the half that is true of one bind only. Asserted on the constant
/// rather than on captured stderr because `disable` cannot be reached
/// without a daemon, and the string is the whole of what this fixes.
///
/// [#1005]: https://github.com/mmogr/gglib/issues/1005
#[test]
fn the_disable_notice_says_the_key_stays_without_claiming_it_can_never_go() {
    let notice = DISABLE_NOTICE.join(" ");

    assert!(
        notice.contains("The API key stays in settings"),
        "the part that holds for every bind is still said: {notice}"
    );
    assert!(
        !notice.contains("never off by itself"),
        "the unqualified claim is gone: {notice}"
    );
    assert!(
        notice.contains("depends"),
        "what replaced it names the bind it depends on: {notice}"
    );
    assert!(
        notice.contains("gglib config settings show"),
        "the operator is pointed at the state rather than left to infer it: {notice}"
    );
}

/// The identity lasts now (ADR 0012, decision 4, reversed), so `disable`
/// must not promise a fresh ticket on the next `enable` — it hands the
/// same one back. Saying otherwise sent people to `disable`/`enable` to
/// rotate an address that does not rotate.
#[test]
fn the_disable_notice_does_not_promise_a_new_ticket() {
    let notice = DISABLE_NOTICE.join(" ");

    assert!(
        !notice.contains("mints a new one"),
        "the claim that `enable` mints a fresh ticket is gone: {notice}"
    );
    assert!(
        notice.contains("the same one back"),
        "what replaced it says the ticket survives: {notice}"
    );
    assert!(
        notice.contains("deleting the endpoint key"),
        "and names what revoking actually is: {notice}"
    );
}

/// The banner is printed a line at a time, so a line that outgrew the
/// terminal would wrap into the two-space indent every other line carries.
#[test]
fn every_line_of_the_disable_notice_fits_a_narrow_terminal() {
    for line in DISABLE_NOTICE {
        assert!(
            line.starts_with("  "),
            "the banner's indent is part of the line: {line:?}"
        );
        assert!(
            line.chars().count() <= 80,
            "{} chars is past an 80-column terminal: {line:?}",
            line.chars().count()
        );
    }
}

/// An `AppCore` over an in-memory database, which is all `switch_off` needs:
/// `disable` itself cannot be reached without a daemon or a whole CLI context.
async fn app() -> AppCore {
    let pool = gglib_db::setup_test_database().await.expect("in-memory DB");
    gglib_db::CoreFactory::build_app_core(pool)
}

/// The state `enable` leaves behind.
async fn switched_on(app: &AppCore) {
    app.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            remote_serve: Some(Some(gglib_core::RemoteServe {
                allow_mcp: true,
                relay: None,
                discovery: true,
            })),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the switch is recorded");
}

/// [#1037]: with no daemon, `disable` used to say nothing was being broadcast
/// and leave the switch on, so the next start put the tunnel back.
///
/// [#1037]: https://github.com/mmogr/gglib/issues/1037
#[tokio::test]
async fn switching_off_with_no_daemon_clears_the_switch_so_the_next_start_does_not_resume() {
    let app = app().await;
    switched_on(&app).await;
    switch_off(&app).await.expect("the switch is cleared");
    let settings = app.settings().get().await.expect("settings");
    assert_eq!(settings.remote_enabled, Some(false));
}

/// Only the switch. The flags `enable` was given stay, as the daemon's own
/// `disable` leaves them, so the two ways of turning it off leave one state.
#[tokio::test]
async fn switching_off_with_no_daemon_leaves_the_serve_flags_alone() {
    let app = app().await;
    switched_on(&app).await;
    switch_off(&app).await.expect("the switch is cleared");
    let serve = app
        .settings()
        .get()
        .await
        .expect("settings")
        .remote_serve
        .expect("the flags stay");
    assert!(
        serve.allow_mcp && serve.discovery,
        "the flags were rewritten"
    );
}

/// The offline notices keep the banner's indent and an 80-column width, say
/// the switch is off, and do not call a port someone else holds a daemon
/// that stopped.
#[test]
fn the_offline_notices_say_the_switch_is_off_and_fit_a_narrow_terminal() {
    for line in [NOT_RUNNING, FOREIGN_SERVER, SWITCHED_OFF] {
        assert!(
            line.starts_with("  "),
            "the banner's indent is part of the line: {line:?}"
        );
        assert!(
            line.chars().count() <= 80,
            "{} chars is past an 80-column terminal: {line:?}",
            line.chars().count()
        );
    }
    assert!(SWITCHED_OFF.contains("switched off"), "{SWITCHED_OFF}");
    assert!(
        SWITCHED_OFF.contains("will not put it back"),
        "{SWITCHED_OFF}"
    );
    assert!(
        !FOREIGN_SERVER.contains("not running"),
        "a port someone else holds is not a daemon that stopped: {FOREIGN_SERVER}"
    );
}
