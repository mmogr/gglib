//! `gglib remote key`: this device's key, printed for a client that is not
//! gglib.
//!
//! The key is the one `join` stored when this device paired. The port `join`
//! binds does not add it, so a third-party client pointed at that port
//! presents it as its API key; gglib's own commands attach it themselves.
//!
//! Stdout carries the key and nothing else, and only under `--show`, so
//! `$(gglib remote key --show)` is exactly the key. Every other line goes to
//! stderr, and this command never writes the key there.

use std::io::Write;

use anyhow::{Result, anyhow, bail};
use gglib_core::RemotePairing;

use crate::bootstrap::CliContext;

/// The refusal when no pairing is stored. It names the command that stores a
/// key, and the setting that holds the other key a person may have meant.
const NO_PAIRING: &str = "no device key is stored here. `gglib remote join <ticket>-<code>` pairs \
     this machine with another and stores one. For this machine's own proxy, the stored key is \
     the `proxy-api-key` setting; `gglib config settings show` prints it";

/// What `--show` says on stderr beside the key.
const SHOWN: &str = "  This device's key for the machine it joined is on stdout. It is a secret: \
     whoever holds it is admitted as this device until that machine retires it with \
     `gglib remote forget`.";

/// What a bare `gglib remote key` says on stderr instead of the key.
const HELD: &str = "  This device holds a key for the machine it joined. `gglib remote key --show` \
     prints it on stdout, alone, for a client that is not gglib.";

/// Execute `gglib remote key`.
pub(super) async fn key(ctx: &CliContext, show: bool) -> Result<()> {
    let settings = ctx
        .app
        .settings()
        .get()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?;
    write_key(
        settings.remote_pairing.as_ref(),
        show,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

/// Write what `gglib remote key` prints for `pairing`: the key to `out` only
/// when `show` is set, and every other line to `err`.
fn write_key(
    pairing: Option<&RemotePairing>,
    show: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<()> {
    let Some(pairing) = pairing else {
        bail!(NO_PAIRING);
    };
    if show {
        writeln!(err, "{SHOWN}")?;
        writeln!(out, "{}", pairing.api_key)?;
    } else {
        writeln!(err, "{HELD}")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;
