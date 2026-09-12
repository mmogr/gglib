//! The pairing screen.
//!
//! Draws the QR and the code in the alternate screen buffer, polls the daemon
//! for a pairing, and leaves the moment one happens, the code expires, or the
//! invite is withdrawn. The alternate buffer is the point: `less` and `vim`
//! draw there so that leaving restores the terminal exactly, and nothing they
//! showed survives in the scrollback. A pairing string is a credential for two
//! minutes; a terminal history is forever.

use std::io::{Write as _, stdout};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{cursor, execute, terminal};

use crate::daemon_client::{DaemonHandle, RemoteEnableDto, RemoteStatusDto};

/// How the screen ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    /// The daemon reported a device paired.
    Paired { peer: Option<String> },
    /// The code expired with nobody pairing.
    Expired,
    /// The daemon let go of the code before it expired, and nobody paired.
    ///
    /// Almost always a withdrawal on this machine — `forget` of the invited
    /// row, or `disable` — but a status cannot say which, and a restart or a
    /// proxy that exited reads the same. `tunnel_up` is whether the serve side
    /// was still up at the read that said so.
    Ended { tunnel_up: bool },
    /// The person pressed Ctrl-C; the tunnel is still up.
    Interrupted,
}

/// How near the end of the countdown a code the daemon has let go of is put
/// down to expiry rather than to a withdrawal.
///
/// The daemon starts its clock when it arms the code, before it answers, and
/// this screen starts its own when the answer arrives, so the daemon always
/// lets go a moment first. A withdrawal inside this window is reported as the
/// expiry it was about to be.
const LAPSE_GRACE: Duration = Duration::from_secs(2);

/// Render the pairing string as a QR code, or `None` if it will not fit.
///
/// Uppercased first, which is not cosmetic: QR alphanumeric mode encodes
/// only uppercase, and using it rather than byte mode makes the code
/// materially smaller and easier for a phone to read. The ticket format
/// parses case-insensitively for exactly this reason, and the six-digit
/// code has no case.
pub(super) fn qr(pairing: &str) -> Option<String> {
    use qrcode::QrCode;
    use qrcode::render::unicode;
    let code = QrCode::new(pairing.to_uppercase()).ok()?;
    Some(code.render::<unicode::Dense1x2>().quiet_zone(true).build())
}

/// What a screen that ended on a withdrawal prints, a line at a time.
///
/// One copy for `enable` and `invite`, which reach the same event under two
/// names. The cause is hedged because the status cannot prove it: `forget` and
/// `disable` are what withdraw a code, but a restart, a proxy that exited, or
/// three wrong codes typed on this machine read the same.
pub(super) fn withdrawn_notice(tunnel_up: bool) -> [&'static str; 2] {
    if tunnel_up {
        [
            "  The invite was withdrawn on this machine (`gglib remote forget`, most \
             likely); nobody paired.",
            "  The tunnel is up and every device already on it is unaffected; \
             `gglib remote invite` offers a fresh code.",
        ]
    } else {
        [
            "  The invite ended with the tunnel (`gglib remote disable`, most likely); \
             nobody paired.",
            "  `gglib remote status` says whether remote access is coming back; if it is \
             off, `gglib remote enable --invite` turns it on with a fresh code.",
        ]
    }
}

/// Show the pairing until a device pairs, the code expires or is withdrawn,
/// or Ctrl-C.
pub(super) async fn run(handle: &DaemonHandle, enabled: &RemoteEnableDto) -> Result<Outcome> {
    // Only reached when `enable` offered a code; the caller guards on it.
    let ttl = Duration::from_secs(enabled.expires_in_s.unwrap_or_default());
    let started = Instant::now();
    let pairing = enabled.pairing.as_deref().unwrap_or_default();
    let rendered = qr(pairing);

    let mut out = stdout();
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    // Whatever happens below, the screen is restored: the guard runs on
    // every exit path, including a `?`.
    let _restore = Restore;

    let mut watch = Watch::default();
    let outcome = loop {
        let left = ttl.saturating_sub(started.elapsed());
        draw(&mut out, enabled, rendered.as_deref(), left)?;
        if left.is_zero() {
            break Outcome::Expired;
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break Outcome::Interrupted,
            () = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        // A read that fails says nothing either way, so it changes nothing.
        // The time left is taken again, because the second's sleep has made
        // the one above stale.
        if let Ok(status) = handle.remote_status().await
            && let Some(outcome) = watch.read(status, ttl.saturating_sub(started.elapsed()))
        {
            break outcome;
        }
    };
    Ok(outcome)
}

/// What the polls have said so far, and when that is enough to leave.
#[derive(Debug, Default)]
struct Watch {
    /// The last read said no code was live and nobody had paired.
    gone: bool,
}

impl Watch {
    /// One status read, with `left` before the countdown ends; `Some` when the
    /// screen should leave.
    ///
    /// It keys on the code being gone, not on its going from live to gone.
    /// The popover keys on the change because it can hold a read from before
    /// its own invite landed. This screen holds none, and the daemon arms a
    /// code before it answers, so every read here comes after the code
    /// existed. Keyed on the change, a withdrawal before the first read would
    /// leave the dead code up for the whole countdown, which is the thing this
    /// exists to stop.
    ///
    /// A gone code has to be read twice running, because a redemption clears
    /// the code and records the pairing in two steps, and a read between them
    /// sees neither. And one that goes inside [`LAPSE_GRACE`] is left for the
    /// countdown to call expired.
    fn read(&mut self, status: RemoteStatusDto, left: Duration) -> Option<Outcome> {
        if status.paired {
            return Some(Outcome::Paired {
                peer: status
                    .last_peer
                    .or_else(|| status.peers.first().map(|p| p.fingerprint.clone())),
            });
        }
        let gone_before = std::mem::replace(&mut self.gone, !status.pairing_active);
        (self.gone && gone_before && left > LAPSE_GRACE).then_some(Outcome::Ended {
            tunnel_up: status.enabled,
        })
    }
}

/// Leaves the alternate screen and shows the cursor again, on drop.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = execute!(stdout(), cursor::Show, terminal::LeaveAlternateScreen);
    }
}

fn draw(
    out: &mut std::io::Stdout,
    enabled: &RemoteEnableDto,
    qr: Option<&str>,
    left: Duration,
) -> Result<()> {
    execute!(
        out,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    writeln!(out, "  gglib remote — pair a device\r")?;
    writeln!(out, "\r")?;
    if let Some(qr) = qr {
        for line in qr.lines() {
            writeln!(out, "  {line}\r")?;
        }
        writeln!(out, "\r")?;
    }
    writeln!(out, "  On the other machine:\r")?;
    writeln!(out, "\r")?;
    let pairing = enabled.pairing.as_deref().unwrap_or_default();
    writeln!(out, "    gglib remote join {pairing}\r")?;
    writeln!(out, "\r")?;
    writeln!(out, "  ticket  {}\r", enabled.ticket)?;
    let code = enabled.code.as_deref().unwrap_or_default();
    writeln!(out, "  code    {code}\r")?;
    writeln!(out, "\r")?;
    writeln!(
        out,
        "  Waiting for a device… the code expires in {}s. Ctrl-C leaves the tunnel up.\r",
        left.as_secs()
    )?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
#[path = "pairing_tui_tests.rs"]
mod pairing_tui_tests;
