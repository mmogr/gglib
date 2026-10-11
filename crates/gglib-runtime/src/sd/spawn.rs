//! Starting `sd-server`: [`build_and_spawn_sd`].
//!
//! The image runtime's twin of `command::build_and_spawn`: the same piped
//! stdio, so `spawn_log_readers` can hand its output to the log manager, and
//! the same `info!` of the whole invocation. Unlike llama-server there is no
//! fallback resolver: the managed path is the only `sd-server` gglib runs.

use std::path::Path;

use gglib_core::paths::SD_INSTALL_COMMAND;
use gglib_core::utils::process::cmd;
use tokio::process::Child;
use tracing::info;

use super::args::sd_server_args;
use super::config::SdServerConfig;

/// Start `sd-server` at `sd_server_path` for `config`, listening on `port`.
///
/// # Errors
///
/// When the binary is not there (naming the install command), or the
/// process cannot be started.
pub(crate) fn build_and_spawn_sd(
    sd_server_path: &Path,
    config: &SdServerConfig,
    port: u16,
) -> anyhow::Result<Child> {
    if !sd_server_path.is_file() {
        anyhow::bail!(
            "sd-server binary not found at: {}\n\nPlease install stable-diffusion.cpp by running:\n  {SD_INSTALL_COMMAND}",
            sd_server_path.display()
        );
    }

    let mut command = cmd(sd_server_path);
    command.args(sd_server_args(config, port));

    info!(
        "spawning sd-server: {} {}",
        command.get_program().to_string_lossy(),
        command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    );

    let mut command = tokio::process::Command::from(command);
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    command
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn sd-server: {e}"))
}
