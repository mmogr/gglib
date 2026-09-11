//! `gglib remote list` and `forget`: who may use this machine's tunnel.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, RemoteDeviceDto};
use crate::presentation::style;

/// Execute `gglib remote list`.
///
/// Works with the tunnel down — the roster is settings, and a person
/// deciding what to retire is often doing it precisely because it is off. In
/// that case `admitted` is unknown rather than false, because nothing is
/// admitted then and saying "no" of one row would read as that device having
/// been singled out.
pub(crate) async fn list(ctx: &CliContext) -> Result<()> {
    let handle =
        daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;
    let devices = handle.remote_devices().await?;

    style::print_info_banner("Devices", "\u{1f4f1}");
    if devices.is_empty() {
        eprintln!("  No device has been paired with this machine.");
        eprintln!("  `gglib remote invite` turns it on and pairs the first one.");
        style::print_banner_close();
        return Ok(());
    }

    let width = devices.iter().map(|d| d.id.len()).max().unwrap_or(2).max(2);
    eprintln!("  {:<width$}  DEVICE", "ID", width = width);
    for d in &devices {
        eprintln!("  {:<width$}  {}", d.id, describe(d), width = width);
    }
    eprintln!();
    eprintln!("  Retire one:  gglib remote forget <id>");
    style::print_banner_close();
    Ok(())
}

/// Execute `gglib remote forget`.
pub(crate) async fn forget(ctx: &CliContext, device: &str) -> Result<()> {
    let handle =
        daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;
    let answer = handle.remote_forget(device).await?;

    if !answer.forgotten {
        // Not an error: what was asked for is that this machine hold no key
        // under that name, and it does not.
        eprintln!("  This machine holds no device called {device}.");
        eprintln!("  `gglib remote list` shows the ids it does hold.");
        return Ok(());
    }
    eprintln!("  \u{2705} Forgot {device}.");
    eprintln!(
        "  Its key stops being admitted from the next request; a reply already \
         streaming to it finishes. Every other device is untouched."
    );
    Ok(())
}

/// One device as a line a person reads.
///
/// **A row is only called never-joined when both timestamps are empty.**
/// `redeemed_at` is written by a background task and can be lost, so a
/// device that has plainly made requests must not be described as one that
/// never arrived — `last_seen` is the second opinion that prevents it.
fn describe(d: &RemoteDeviceDto) -> String {
    let name = d.label.as_deref().unwrap_or("\u{2014}");
    if d.redeemed_at.is_none() && d.last_seen.is_none() {
        return format!("{name}  (invited {}, never joined)", ago(d.joined_at));
    }
    let seen = match d.last_seen {
        Some(at) => format!("last seen {}", ago(at)),
        None => "no requests yet".to_owned(),
    };
    match d.admitted {
        Some(true) => format!("{name}  ({seen})"),
        Some(false) => format!("{name}  ({seen}; not admitted)"),
        // The tunnel is down, so nothing is admitted and this row is not
        // special for it.
        None => format!("{name}  ({seen}; tunnel down)"),
    }
}

/// A coarse "how long ago", for a column a person scans rather than measures.
fn ago(at_ms: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0);
    let secs = (now - at_ms) / 1000;
    if secs < 0 {
        // A clock that moved backwards. "In the future" is a wrong answer a
        // person can act on; a silent negative is not.
        return "at an unknown time".to_owned();
    }
    match secs {
        s if s < 60 => "just now".to_owned(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod devices_tests;
