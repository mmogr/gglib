//! Tests for the pairing screen: what a status read means, and what the
//! screen says when a withdrawal ends it.
//!
//! A `#[path]` sibling because the screen and its tests together would come
//! too close to the 300-line budget.

use std::time::Duration;

use super::*;

/// A status as the daemon's JSON has it. Built from JSON rather than a struct
/// literal, so the serde defaults the screen relies on are part of what is
/// under test.
fn status(enabled: bool, pairing_active: bool, paired: bool) -> RemoteStatusDto {
    serde_json::from_value(serde_json::json!({
        "enabled": enabled,
        "pairing_active": pairing_active,
        "paired": paired,
    }))
    .expect("a status the daemon could send")
}

/// Well inside the countdown.
const EARLY: Duration = Duration::from_secs(60);

/// A ticket from modelpipe's format vectors plus a code, as `enable`
/// would print it, fits a QR and round-trips through uppercasing.
#[test]
fn the_pairing_string_fits_a_qr_when_upper_cased() {
    let pairing = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na-483920";
    let rendered = qr(pairing).expect("fits");
    assert!(rendered.lines().count() > 10, "a drawn code has rows");
}

/// `forget` or `disable` withdrawing the code ends the screen, rather than
/// leaving a code nobody can redeem on it until the countdown calls it
/// expired.
#[test]
fn a_code_withdrawn_before_it_expires_ends_the_screen() {
    let mut watch = Watch::default();
    assert_eq!(watch.read(status(true, true, false), EARLY), None);
    assert_eq!(
        watch.read(status(true, false, false), EARLY),
        None,
        "one read of a gone code is not enough"
    );
    assert_eq!(
        watch.read(status(true, false, false), EARLY),
        Some(Outcome::Ended { tunnel_up: true })
    );
}

/// `disable` takes the tunnel down with the code, and the screen says which
/// of the two it saw.
#[test]
fn a_withdrawal_that_took_the_tunnel_down_says_so() {
    let mut watch = Watch::default();
    assert_eq!(watch.read(status(false, false, false), EARLY), None);
    assert_eq!(
        watch.read(status(false, false, false), EARLY),
        Some(Outcome::Ended { tunnel_up: false })
    );
}

/// A redemption clears the code and then records the pairing, and a read can
/// land between the two. The second read is what tells that from a
/// withdrawal; without it, a device that had just paired would be told that
/// nobody did.
#[test]
fn a_redemption_read_between_its_two_writes_is_still_a_pairing() {
    let mut watch = Watch::default();
    assert_eq!(watch.read(status(true, false, false), EARLY), None);
    assert_eq!(
        watch.read(status(true, false, true), EARLY),
        Some(Outcome::Paired { peer: None })
    );
}

/// The daemon's clock starts before this screen's, so a code at the end of
/// its life is let go a moment early. That is an expiry, and it is left to
/// the countdown to say so.
#[test]
fn a_code_the_daemon_let_go_of_first_is_left_to_the_countdown() {
    let mut watch = Watch::default();
    let gone = || status(true, false, false);
    assert_eq!(watch.read(gone(), Duration::from_millis(1800)), None);
    assert_eq!(watch.read(gone(), Duration::from_millis(800)), None);
}

/// A live read between two gone ones starts the count again.
#[test]
fn a_live_read_between_two_gone_reads_starts_the_count_again() {
    let mut watch = Watch::default();
    assert_eq!(watch.read(status(true, false, false), EARLY), None);
    assert_eq!(watch.read(status(true, true, false), EARLY), None);
    assert_eq!(watch.read(status(true, false, false), EARLY), None);
}

/// With the tunnel down, `invite` would be refused, so the notice does not
/// send anyone to it; with the tunnel up, it is the command that works.
#[test]
fn the_withdrawn_notice_with_the_tunnel_down_does_not_send_anyone_to_invite() {
    let down = withdrawn_notice(false).join(" ");
    assert!(down.contains("`gglib remote status`"), "{down}");
    assert!(!down.contains("`gglib remote invite`"), "{down}");

    let up = withdrawn_notice(true).join(" ");
    assert!(up.contains("`gglib remote invite`"), "{up}");
}
