//! `gglib config settings reveal <key>`: a stored secret, alone on stdout.
//!
//! The one place the CLI prints a stored secret (ADR 0027); every other
//! surface says only whether it is held. Stdout carries the value and nothing
//! else, so `$(gglib config settings reveal proxy-api-key)` is exactly the
//! key. The warning that it is a secret goes to stderr, which never carries
//! the value. With nothing stored the command fails and prints nothing on
//! stdout.

use std::io::Write;

use anyhow::{Result, bail};
use gglib_core::secret::{SecretRef, SettingSecret};

use crate::bootstrap::CliContext;

/// Execute `gglib config settings reveal <secret>`.
pub(super) async fn handle_reveal(ctx: &CliContext, secret: SettingSecret) -> Result<()> {
    let value = ctx
        .app
        .secrets()
        .reveal(&SecretRef::Setting { secret })
        .await?;
    write_value(
        secret,
        value.as_deref(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
    )
}

/// Write `value` alone to `out`, and what it is to `err`; refuse when
/// nothing is stored.
fn write_value(
    secret: SettingSecret,
    value: Option<&str>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<()> {
    let Some(value) = value else {
        bail!(nothing_stored(secret));
    };
    writeln!(err, "{}", shown(secret))?;
    writeln!(out, "{value}")?;
    Ok(())
}

/// What stderr says beside the value.
const fn shown(secret: SettingSecret) -> &'static str {
    match secret {
        SettingSecret::ProxyApiKey => {
            "  This machine's proxy key is on stdout. It is a secret: whoever holds it can use \
             this machine's proxy, and its daemon when that is shared on the network."
        }
        SettingSecret::RemotePairingKey => {
            "  This device's key for the machine it joined is on stdout. It is a secret: \
             whoever holds it is admitted as this device until that machine retires it with \
             `gglib remote forget`."
        }
    }
}

/// The refusal when nothing is stored: what stores one, and the other key a
/// person may have meant.
fn nothing_stored(secret: SettingSecret) -> String {
    match secret {
        SettingSecret::ProxyApiKey => format!(
            "no proxy key is stored. `gglib config settings set --proxy-api-key <key>` stores \
             one, and a proxy or daemon bound off loopback mints one. The key this device was \
             given when it paired is `{}`",
            SettingSecret::RemotePairingKey.name()
        ),
        SettingSecret::RemotePairingKey => format!(
            "no device key is stored here. `gglib remote join <ticket>-<code>` pairs this \
             machine with another and stores one. For this machine's own proxy, the key is \
             `{}`: `{}` prints it",
            SettingSecret::ProxyApiKey.name(),
            SettingSecret::ProxyApiKey.reveal_command()
        ),
    }
}

#[cfg(test)]
#[path = "reveal_tests.rs"]
mod tests;
