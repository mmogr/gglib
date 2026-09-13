//! `gglib remote disable`.
//!
//! Its own file since it learned to switch remote access off with no daemon
//! running, which would have put `mod.rs` past its budget. What it prints is
//! pinned by `disable_tests.rs`.

use std::time::Duration;

use anyhow::{Context as _, Result};
use gglib_core::SettingsUpdate;
use gglib_core::services::AppCore;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonProbe};

/// Execute `gglib remote disable`.
///
/// With no gglib daemon to ask, the switch is cleared here instead, so the
/// next start does not put the tunnel back ([#1037]). This used to print that
/// nothing was being broadcast and leave the switch on, and the daemon's next
/// start then resumed the tunnel it had just been asked to stop.
///
/// [#1037]: https://github.com/mmogr/gglib/issues/1037
pub(crate) async fn disable(ctx: &CliContext) -> Result<()> {
    let client = reqwest::Client::new();
    // Asked twice before anything is written: the ordinary probe gives the
    // daemon half a second, which one busy arming a tunnel can miss, and
    // writing the switch off behind a daemon that is running would say "not
    // running" of it while its tunnel stayed up.
    let mut found = daemon_client::probe(&client).await;
    if matches!(found, DaemonProbe::NotRunning) {
        found = daemon_client::probe_within(&client, PATIENT_PROBE).await;
    }
    let nobody_answered = match found {
        DaemonProbe::Running => None,
        DaemonProbe::NotRunning => Some(NOT_RUNNING),
        DaemonProbe::ForeignServer => Some(FOREIGN_SERVER),
    };
    if let Some(first) = nobody_answered {
        switch_off(&ctx.app).await?;
        eprintln!("{first}");
        eprintln!("{SWITCHED_OFF}");
        return Ok(());
    }
    let handle = daemon_client::DaemonHandle {
        client,
        api_key: daemon_client::auth::daemon_api_key(ctx).await,
    };
    let status = handle.remote_disable().await?;
    if status.enabled {
        anyhow::bail!("the daemon reported remote access still enabled after disable");
    }
    for line in DISABLE_NOTICE {
        eprintln!("{line}");
    }
    Ok(())
}

/// Clear the switch with no daemon to ask.
///
/// The same write the daemon's own `disable` makes first, and only that. The
/// flags `enable` was given stay, as they do there, because only `resume`
/// reads them and it reads them only with the switch on.
///
/// A daemon starting at the same moment may already have read the switch as
/// on, and resume anyway. `gglib remote disable` against that daemon is the
/// answer then, as it always was.
pub(super) async fn switch_off(app: &AppCore) -> Result<()> {
    app.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(false)),
            ..SettingsUpdate::default()
        })
        .await
        .context("could not switch remote access off in settings")?;
    Ok(())
}

/// How long the second ask gives the daemon before `disable` concludes that
/// none is running and writes the switch itself.
const PATIENT_PROBE: Duration = Duration::from_secs(3);

/// The first line when nothing answered on the daemon's port.
const NOT_RUNNING: &str = "  Daemon is not running \u{2014} nothing is being broadcast.";

/// The first line when something else holds the port: no gglib daemon is
/// running there either, but "not running" would be the wrong sentence.
const FOREIGN_SERVER: &str =
    "  Another program holds the daemon port \u{2014} no gglib daemon is broadcasting.";

/// What either of those means for the switch, said after it.
const SWITCHED_OFF: &str =
    "  Remote access is switched off, so starting the daemon will not put it back.";

/// What `gglib remote disable` prints once the tunnel is down.
///
/// A constant so the wording is pinned by a test rather than by nobody. The
/// sentence this replaced — "authentication turns on and never off by itself"
/// — is true of a listener that bound with a key already in settings, and
/// false of the ordinary case this command ends: `enable` mints the key into a
/// proxy that is *already* bound on loopback, and a loopback bind resolves no
/// key, so that listener has no bind-time floor and clearing `proxy_api_key`
/// reopens it, `/mcp` included. `clearing_reopens_a_listener_that_bound_on_loopback`
/// in `gglib-core`'s `access::bearer_tests` asserts exactly that.
///
/// Which of the two a running listener is, this side cannot see: `disable`
/// talks to the daemon over HTTP and never learns the proxy's bind. So the
/// notice names the dependency and points at the command that shows the state,
/// rather than asserting a rule that holds in one case only. `docs/remote.md`
/// carries both cases in full, and ADR 0012 the reasoning.
const DISABLE_NOTICE: [&str; 4] = [
    "  Remote access is off. Nothing answers the ticket while it is off, and",
    "  `enable` brings the same one back \u{2014} revoking is deleting the endpoint key.",
    "  The API key stays in settings \u{2014} whether the proxy still demands it depends",
    "  on the bind its listener came up with. `gglib config settings show` prints it.",
];

#[cfg(test)]
#[path = "disable_tests.rs"]
mod disable_tests;
