use crate::utils::paths::{get_llama_cli_path, get_llama_server_path};
use anyhow::Result;
use std::io::{self, Write};

/// Ensure that llama.cpp binaries are installed.
///
/// Checks for the existence of `llama-server` and `llama-cli`.
/// If missing, prompts the user to install them automatically.
pub async fn ensure_llama_initialized() -> Result<()> {
    let server_path = get_llama_server_path()?;
    let cli_path = get_llama_cli_path()?;

    if server_path.exists() && cli_path.exists() {
        return Ok(());
    }

    println!();
    println!("⚠️  llama.cpp binaries not found.");
    println!("   Server path: {}", server_path.display());
    println!("   CLI path:    {}", cli_path.display());
    println!();
    print!("Would you like to install them now? [Y/n] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("n") {
        anyhow::bail!(
            "llama.cpp is required to run this command. Run 'gglib llama install' manually."
        );
    }

    println!("Installing llama.cpp (auto-detecting hardware)...");

    // Call the install handler with auto-detect flags (all false triggers auto-detect)
    super::handle_install(false, false, false, false).await?;

    Ok(())
}
