//! The optional `hf_xet` download accelerator.
//!
//! Downloads work without any of this — the native Rust HTTP path is the
//! default and the fallback. The accelerator is a Python environment gglib
//! builds and owns under its own data directory; enabling it is the only thing
//! here that writes anything, and the environment is never provisioned
//! implicitly by a download.
//!
//! Nothing in this module requires the user to adopt a particular Python
//! toolchain. gglib finds an interpreter, builds its own environment from it,
//! and scrubs the environment variables of whatever conda or venv the user has
//! active before running anything.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Context, Result};
use gglib_download::cli_exec::{
    FastHelperStatus, ensure_fast_helper_ready, ensure_fast_helper_ready_with_python,
    fast_helper_status, preflight_fast_helper, remove_fast_helper,
};

use crate::config_commands::FastDownloadsCommand;
use crate::presentation::style::{BOLD, INFO, RESET, SUCCESS, WARNING};
use crate::utils::input::prompt_confirmation_default_yes;

/// Preseeds the interactive offer, for CI and scripted installs.
///
/// `1`/`true`/`yes` enables without asking, anything else declines without
/// asking. Absent means ask, if there is somebody to ask.
const PRESEED_ENV: &str = "GGLIB_FAST_DOWNLOADS";

/// Dispatch a `config fast-downloads` subcommand.
pub(crate) async fn dispatch(command: Option<FastDownloadsCommand>) -> Result<()> {
    match command.unwrap_or(FastDownloadsCommand::Status) {
        FastDownloadsCommand::Status => status(),
        FastDownloadsCommand::Enable { python } => enable(python.as_deref()).await,
        FastDownloadsCommand::Disable => disable(),
        FastDownloadsCommand::Prompt => prompt().await,
    }
}

/// Report what is on disk.
fn status() -> Result<()> {
    let status = fast_helper_status().context("Failed to inspect the download accelerator")?;

    if status.provisioned {
        println!("{}✓ Fast downloads are enabled{}", SUCCESS, RESET);
        println!("  environment  {}", status.env_dir.display());
        if let Some(builder) = &status.builder {
            println!("  built with   {builder}");
        }
        if status.legacy_path {
            println!(
                "  {}note{}         at the pre-0.13 location; it keeps working, and \
                 re-enabling after `disable` moves it",
                WARNING, RESET
            );
        }
    } else {
        println!("{}○ Fast downloads are not enabled{}", INFO, RESET);
        println!("  downloads run natively over HTTP, which always works");
        println!("  would install to  {}", status.env_dir.display());
        println!("  would build with  {}", status.available_builder);
        println!();
        println!(
            "{}Enable with:{} gglib config fast-downloads enable",
            BOLD, RESET
        );
    }

    Ok(())
}

/// Build the environment.
async fn enable(python: Option<&str>) -> Result<()> {
    println!("{}Provisioning the download accelerator...{}", BOLD, RESET);

    ensure_fast_helper_ready_with_python(python.map(Path::new))
        .await
        .context("Failed to set up the download accelerator")?;

    println!("{}✓ Fast downloads are enabled{}", SUCCESS, RESET);
    Ok(())
}

/// Remove the environment.
fn disable() -> Result<()> {
    let removed = remove_fast_helper().context("Failed to remove the download accelerator")?;

    if removed {
        println!("{}✓ Fast downloads are disabled{}", SUCCESS, RESET);
        println!("  downloads now run natively over HTTP");
    } else {
        println!("{}○ Fast downloads were already disabled{}", INFO, RESET);
    }

    Ok(())
}

/// Offer to enable it.
///
/// This runs inside `make setup`, so it must never fail: a user who does not
/// want the accelerator, or whose machine cannot build it, still has a
/// successful setup. Every path here returns `Ok`.
async fn prompt() -> Result<()> {
    let status = fast_helper_status().context("Failed to inspect the download accelerator")?;

    if status.provisioned {
        println!("{}✓ Fast downloads are already enabled{}", SUCCESS, RESET);
        return Ok(());
    }

    match preseed() {
        Some(true) => return provision_or_explain().await,
        Some(false) => {
            println!(
                "{}Skipping the download accelerator ({PRESEED_ENV}).{}",
                INFO, RESET
            );
            return Ok(());
        }
        None => {}
    }

    if !std::io::stdin().is_terminal() {
        // No terminal is not an answer. Say what did not happen and how to do
        // it later, rather than assuming either way on the user's behalf.
        println!(
            "{}Downloads run natively over HTTP. For the faster hf_xet path, run:{} \
             gglib config fast-downloads enable",
            INFO, RESET
        );
        return Ok(());
    }

    // Probe before offering: there is no point asking a question whose answer
    // we cannot act on, and "no Python" is a fact worth telling the user
    // plainly rather than surfacing as a failed install.
    if let Err(e) = preflight_fast_helper().await {
        println!(
            "{}○ Skipping the optional download accelerator.{}",
            INFO, RESET
        );
        println!("  {e}");
        println!("  Downloads will run natively over HTTP, which always works.");
        println!("  Install Python 3.9+ and run `gglib config fast-downloads enable` to add it.");
        return Ok(());
    }

    describe(&status);

    match prompt_confirmation_default_yes("Enable fast downloads?") {
        Ok(true) => provision_or_explain().await,
        Ok(false) => {
            println!(
                "{}Skipped. Downloads will run natively over HTTP.{}",
                INFO, RESET
            );
            println!("  Change your mind with `gglib config fast-downloads enable`.");
            Ok(())
        }
        Err(e) => {
            // A failed read is not a "no", but there is nothing else to do
            // with it here, and setup must continue.
            println!("{}Could not read a reply ({e}); skipping.{}", INFO, RESET);
            Ok(())
        }
    }
}

/// What the user is agreeing to, before they agree to it.
fn describe(status: &FastHelperStatus) {
    println!();
    println!(
        "{}Fast downloads{} — HuggingFace's hf_xet transfer, noticeably quicker \
         than plain HTTP for large GGUFs.",
        BOLD, RESET
    );
    println!(
        "  gglib builds its own Python environment for this, using {}.",
        status.available_builder
    );
    println!(
        "  It goes in {} and nothing else on",
        status.env_dir.display()
    );
    println!("  your system is touched — no packages are installed outside it.");
    println!("  Remove it any time with `gglib config fast-downloads disable`.");
    println!();
}

/// Provision, reporting failure as a skipped optional step.
///
/// The accelerator is optional by construction, so a failure to build it is
/// worth reporting but is not worth failing the surrounding command over.
async fn provision_or_explain() -> Result<()> {
    println!("{}Provisioning the download accelerator...{}", BOLD, RESET);

    match ensure_fast_helper_ready().await {
        Ok(()) => {
            println!("{}✓ Fast downloads are enabled{}", SUCCESS, RESET);
        }
        Err(e) => {
            println!(
                "{}○ Could not set up the download accelerator:{} {e}",
                WARNING, RESET
            );
            println!("  Downloads will run natively over HTTP, which always works.");
            println!("  Retry with `gglib config fast-downloads enable`.");
        }
    }

    Ok(())
}

/// The preseeded answer, if there is one.
///
/// Split out and pure so the accepted spellings are pinned by tests: this is
/// the interface CI and install scripts hold onto.
fn parse_preseed(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on"
    )
}

fn preseed() -> Option<bool> {
    std::env::var(PRESEED_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(|v| parse_preseed(&v))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These spellings are what an install script or CI job will reach for,
    /// and getting one wrong silently means the opposite of what was asked.
    #[test]
    fn preseed_accepts_the_usual_affirmatives() {
        for value in ["1", "true", "yes", "y", "on", "TRUE", "Yes", " 1 "] {
            assert!(parse_preseed(value), "{value:?} should enable");
        }
    }

    /// Anything not recognised as yes declines. A typo must not enable
    /// something unattended — the whole point of the preseed is that nobody
    /// is watching.
    #[test]
    fn preseed_treats_anything_else_as_no() {
        for value in ["0", "false", "no", "n", "off", "maybe", "tru"] {
            assert!(!parse_preseed(value), "{value:?} should decline");
        }
    }
}
