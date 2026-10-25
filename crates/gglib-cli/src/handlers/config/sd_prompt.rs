//! `gglib config sd prompt`: the offer `make setup` makes to install the
//! image runtime, stable-diffusion.cpp's `sd-server`.
//!
//! The twin of `fast-downloads prompt`, and optional as that is: chat models
//! are served without it. It asks only somebody at a terminal, defaults to
//! no, and installs with `config sd install`'s own function, so the choice
//! between a download and a source build and the progress are that
//! command's. A failed install is a skipped step, and setup goes on.

use std::future::Future;
use std::io::{BufRead, BufReader, IsTerminal, Write};

use anyhow::Result;
use gglib_core::paths::{SD_INSTALL_COMMAND, sd_server_path};

use super::engine;
use crate::bootstrap::CliContext;
use crate::presentation::style::{BOLD, INFO, RESET, SUCCESS, WARNING};
use crate::utils::input::confirm_from;
use gglib_core::domain::RuntimeKind;

/// Where the web page and the desktop app install it.
const IN_SETTINGS: &str = "Settings \u{2192} System \u{2192} Image runtime";

/// Offer to install the image runtime to whoever is at this terminal.
pub(super) async fn prompt(ctx: &CliContext) -> Result<()> {
    let installed = sd_server_path()?.exists();
    let stdin = std::io::stdin();
    let terminal = stdin.is_terminal();
    offer(
        installed,
        terminal,
        &mut BufReader::new(stdin),
        &mut std::io::stdout(),
        // Not forced: nothing is installed, and a source build, where no
        // release fits this machine, says what it will do and asks itself.
        || engine::install(ctx, RuntimeKind::StableDiffusion, false, false),
    )
    .await
}

/// The offer: say `installed` and ask nothing; with no `terminal`, say how to
/// install it and ask nothing; else describe it, ask on `out` and read the
/// answer from `input`, and on a yes run `install`.
///
/// This runs inside `make setup`, so an answer of no, nobody to ask, and an
/// install that failed all return `Ok`: the image runtime is optional, and
/// a setup without it is a successful one.
async fn offer<F>(
    installed: bool,
    terminal: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
    install: impl FnOnce() -> F,
) -> Result<()>
where
    F: Future<Output = Result<()>>,
{
    if installed {
        writeln!(
            out,
            "{SUCCESS}\u{2713} The image runtime is already installed{RESET}"
        )?;
        return Ok(());
    }
    if !terminal {
        // No terminal is not an answer: say what did not happen and how to
        // do it later.
        writeln!(
            out,
            "{INFO}The image runtime is not installed. To draw pictures with an image model, \
             run:{RESET} {SD_INSTALL_COMMAND}"
        )?;
        return Ok(());
    }

    writeln!(out)?;
    writeln!(
        out,
        "{BOLD}Image runtime{RESET} \u{2014} stable-diffusion.cpp's sd-server, which draws \
         pictures with an image model."
    )?;
    writeln!(
        out,
        "  Optional: chat models work without it. Installing downloads its pre-built release"
    )?;
    writeln!(
        out,
        "  for this machine, or builds it from source where there is none."
    )?;
    writeln!(out)?;

    match confirm_from(input, out, "Install the image runtime now?", false) {
        Ok(true) => {
            if let Err(e) = install().await {
                writeln!(
                    out,
                    "{WARNING}\u{25cb} Could not install the image runtime:{RESET} {e:#}"
                )?;
                writeln!(out, "  Skipped. Chat models work without it.")?;
                writeln!(
                    out,
                    "  Retry with `{SD_INSTALL_COMMAND}`, or in {IN_SETTINGS}."
                )?;
            }
        }
        Ok(false) => {
            writeln!(out, "{INFO}Skipped the image runtime.{RESET}")?;
            writeln!(
                out,
                "  Install it later with `{SD_INSTALL_COMMAND}`, or in {IN_SETTINGS}."
            )?;
        }
        Err(e) => {
            // A failed read is not a no, but there is nothing else to do
            // with it here, and setup must continue.
            writeln!(out, "{INFO}Could not read a reply ({e}); skipping.{RESET}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "sd_prompt_tests.rs"]
mod tests;
