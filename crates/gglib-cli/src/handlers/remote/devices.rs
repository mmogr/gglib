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
        eprintln!("  `gglib remote enable --invite` turns it on and pairs the first one.");
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
    // Checked here because this is the only place the id is not already a
    // device id: everywhere else it came from the roster, and here it came
    // from a person's shell. It is interpolated into a request path, and
    // reqwest resolves dot-segments the way a browser does, so an id with a
    // `..` in it does not fail — it reaches a *different route*. Saying "not
    // a device id" is both the safe answer and the useful one, since the
    // alternative is a 405 from somewhere the person never named.
    if !is_device_id(device) {
        // Quoted, because by construction this string failed the charset —
        // it can carry escape sequences, a newline, or ten kilobytes, and it
        // is about to be printed to a terminal.
        //
        // And an error, not `Ok`: "this machine holds no device by that
        // name" is a success because the outcome asked for was reached, and
        // that argument does not carry over to a name that could not name a
        // device at all. `forget "$id" && echo retired` must not say retired.
        anyhow::bail!(
            "{device:?} is not a device id.\n               Ids look like dev-0a1b2c3d; `gglib remote list` shows this machine's."
        );
    }

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

/// The shape every id this machine mints has.
///
/// Narrower than the edge by one rule: modelpipe's `valid_name` is non-empty,
/// at most 64, and `[A-Za-z0-9._-]`, which admits `.` and `..` — and those
/// are the whole of the problem here, because this value is interpolated
/// into a request path. So an alphanumeric is required as well. Nothing this
/// machine mints is excluded by it.
///
/// Deliberately a shape test rather than a roster lookup: an id that is not
/// one cannot be held, so there is nothing to ask the daemon, and refusing
/// it here means the roster's own answer ("this machine holds no device
/// called …") keeps its one meaning.
///
/// [`forget`] calls this before it builds a path. Its two tests below reach
/// it directly, so they do not pin that call — the guarantee they give is
/// about the predicate, and the call site is one line with a `bail!` under
/// it.
fn is_device_id(device: &str) -> bool {
    device.len() <= 64
        && device
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        // At least one letter or digit, which is what rules out `.` and
        // `..` — both of which pass the charset above, and both of which
        // are the whole of the problem. It also covers the empty string.
        && device.bytes().any(|b| b.is_ascii_alphanumeric())
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
