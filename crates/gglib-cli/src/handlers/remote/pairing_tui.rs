//! The pairing screen.
//!
//! Draws the QR and the code in the alternate screen buffer, polls the daemon
//! for a pairing, and leaves the moment one happens or the code expires. The
//! alternate buffer is the point: `less` and `vim` draw there so that leaving
//! restores the terminal exactly, and nothing they showed survives in the
//! scrollback. A pairing string is a credential for two minutes; a terminal
//! history is forever.

use std::io::{Write as _, stdout};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{cursor, execute, terminal};

use crate::daemon_client::{DaemonHandle, RemoteEnableDto};

/// How the screen ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    /// The daemon reported a device paired.
    Paired { peer: Option<String> },
    /// The code expired with nobody pairing.
    Expired,
    /// The person pressed Ctrl-C; the tunnel is still up.
    Interrupted,
}

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

/// Show the pairing until a device pairs, the code expires, or Ctrl-C.
pub(super) async fn run(handle: &DaemonHandle, enabled: &RemoteEnableDto) -> Result<Outcome> {
    let ttl = Duration::from_secs(enabled.expires_in_s);
    let started = Instant::now();
    let rendered = qr(&enabled.pairing);

    let mut out = stdout();
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
    // Whatever happens below, the screen is restored: the guard runs on
    // every exit path, including a `?`.
    let _restore = Restore;

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
        if let Ok(status) = handle.remote_status().await
            && status.paired
        {
            break Outcome::Paired {
                peer: status
                    .last_peer
                    .or_else(|| status.peers.first().map(|p| p.fingerprint.clone())),
            };
        }
    };
    Ok(outcome)
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
    writeln!(out, "    gglib remote connect {}\r", enabled.pairing)?;
    writeln!(out, "\r")?;
    writeln!(out, "  ticket  {}\r", enabled.ticket)?;
    writeln!(out, "  code    {}\r", enabled.code)?;
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
mod tests {
    use super::*;

    /// A ticket from modelpipe's format vectors plus a code, as `enable`
    /// would print it, fits a QR and round-trips through uppercasing.
    #[test]
    fn the_pairing_string_fits_a_qr_when_upper_cased() {
        let pairing = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na-483920";
        let rendered = qr(pairing).expect("fits");
        assert!(rendered.lines().count() > 10, "a drawn code has rows");
    }
}
