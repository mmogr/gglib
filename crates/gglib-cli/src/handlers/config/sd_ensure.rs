//! The install `gglib serve` offers when it is asked for an image model and
//! finds no `sd-server`: the twin of `llama_ensure`.
//!
//! It says what it found, asks, and installs the way `config sd install`
//! would on this machine, with the same choice between a download and a
//! build and the same progress.

use anyhow::{Result, bail};
use gglib_core::paths::{SD_INSTALL_COMMAND, sd_server_path};

use super::sd::{PRODUCT, install};
use crate::utils::input;

/// Ensure `sd-server` is installed before `gglib serve` loads an image
/// model, installing it if the user agrees.
///
/// Declining is an error, as for llama.cpp: the command cannot go on
/// without the runtime. The end of input is a no.
pub(crate) async fn ensure_installed() -> Result<()> {
    let server_path = sd_server_path()?;
    if server_path.exists() {
        return Ok(());
    }
    println!();
    println!("\u{26a0}\u{fe0f}  {PRODUCT} is not installed; image models need its sd-server.");
    println!("   Server path: {}", server_path.display());
    println!();
    if !input::prompt_confirmation_default_yes(&format!(
        "Would you like to install {PRODUCT} now?"
    ))? {
        bail!(not_installed_refusal());
    }
    // The yes was given above, so a source build does not ask again.
    install(true, false).await
}

/// What `gglib serve` says when the user declines the install.
fn not_installed_refusal() -> String {
    format!("{PRODUCT} is required to serve an image model. Run '{SD_INSTALL_COMMAND}' manually.")
}

#[cfg(test)]
mod tests {
    #[test]
    fn declining_the_install_serve_offers_names_the_command() {
        assert_eq!(
            super::not_installed_refusal(),
            "stable-diffusion.cpp is required to serve an image model. \
             Run 'gglib config sd install' manually."
        );
    }
}
