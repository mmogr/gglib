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
fn status(enabled: bool, pairing_active: bool, paired: bool) -> RemoteStatus {
    serde_json::from_value(serde_json::json!({
        "enabled": enabled,
        "pairing_active": pairing_active,
        "paired": paired,
    }))
    .expect("a status the daemon could send")
}

/// An enable answer that offered a code, as the daemon's JSON has it,
/// naming `device` or, like a daemon that predates per-device keys, none.
/// Built from JSON for the same reason `status` is.
fn offer(device: Option<&str>) -> RemoteEnableResponse {
    let mut answer = serde_json::json!({
        "ticket": "pipeaaaa",
        "code": "483920",
        "pairing": "pipeaaaa-483920",
        "expires_in_s": 120,
        "mcp_allowed": false,
    });
    if let Some(device) = device {
        answer["device"] = device.into();
    }
    serde_json::from_value(answer).expect("an answer the daemon could send")
}

/// A watch on a code whose answer named no device, built the way `run`
/// builds one.
fn watch() -> Watch {
    Watch::for_offer(&offer(None))
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
    let mut watch = watch();
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
    let mut watch = watch();
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
    let mut watch = watch();
    assert_eq!(watch.read(status(true, false, false), EARLY), None);
    assert_eq!(
        watch.read(status(true, false, true), EARLY),
        Some(Outcome::Paired { device: None })
    );
}

/// The screen names the device the code was minted for, not the endpoint the
/// status last heard from. That endpoint can be another device's, talking
/// while this one pairs, and it is a fingerprint, which neither `list` nor
/// `forget` takes.
#[test]
fn a_pairing_names_the_device_its_code_was_for_not_the_last_peer() {
    let enabled = offer(Some("dev-a1b2c3d4"));
    let paired: RemoteStatus = serde_json::from_value(serde_json::json!({
        "enabled": true,
        "paired": true,
        "last_peer": "0123456789ab",
        "peers": [{ "fingerprint": "ba9876543210", "path": "relay" }],
    }))
    .expect("a status the daemon could send");

    let mut watch = Watch::for_offer(&enabled);

    assert_eq!(
        watch.read(paired, EARLY),
        Some(Outcome::Paired {
            device: Some("dev-a1b2c3d4".to_owned())
        })
    );
}

/// What a pairing prints names the device by the id `list` shows and
/// `forget` takes.
#[test]
fn the_pairing_lines_name_the_device_by_the_id_forget_takes() {
    let paired = paired_line(Some("dev-a1b2c3d4"));
    assert!(paired.contains("Paired dev-a1b2c3d4."), "{paired}");
    assert!(paired.contains("`gglib remote list`"), "{paired}");
    assert_eq!(paired_line(None), "  \u{2705} A device paired.");

    let plain = code_is_for("dev-a1b2c3d4");
    assert!(
        plain.contains("`gglib remote forget dev-a1b2c3d4`"),
        "{plain}"
    );
}

/// The daemon's clock starts before this screen's, so a code at the end of
/// its life is let go a moment early. That is an expiry, and it is left to
/// the countdown to say so.
#[test]
fn a_code_the_daemon_let_go_of_first_is_left_to_the_countdown() {
    let mut watch = watch();
    let gone = || status(true, false, false);
    assert_eq!(watch.read(gone(), Duration::from_millis(1800)), None);
    assert_eq!(watch.read(gone(), Duration::from_millis(800)), None);
}

/// A live read between two gone ones starts the count again.
#[test]
fn a_live_read_between_two_gone_reads_starts_the_count_again() {
    let mut watch = watch();
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
