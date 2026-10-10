//! The image runtime, stable-diffusion.cpp's `sd-server`, as the daemon
//! installs, reports and removes it: the web face of `gglib config sd
//! install|status|uninstall`.
//!
//! A `#[path]` child of `setup.rs`, so it adds to [`SetupOps`] beside the
//! llama.cpp install it mirrors. Only the pre-built install is offered here,
//! as only llama.cpp's is: a source build streams a different event type and
//! takes the command line.

use serde::Serialize;

use gglib_core::domain::RuntimeKind;
use gglib_core::paths::SD_INSTALL_COMMAND;
use gglib_core::ports::ModelRuntimePort;
use gglib_runtime::llama::{LlamaProgressEvent, UninstallOutcome};
use gglib_runtime::sd::{SdStatus, check_sd_prebuilt_availability, sd_status, uninstall_sd};

use super::SetupOps;
use crate::error::GuiError;

/// What the Settings page says about the image runtime.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ImageRuntimeStatus {
    /// What is installed, as `gglib config sd status` reports it.
    pub install: SdStatus,
    /// The pre-built build an install downloads on this machine
    /// (`macOS universal (Metal)`); null when there is none.
    pub prebuilt: Option<String>,
    /// Why there is no pre-built build, when there is none; the command line
    /// then builds from source.
    pub prebuilt_unavailable: Option<String>,
    /// Something to tell the user about that build: the CPU build's "images
    /// will take minutes each".
    pub warning: Option<String>,
    /// The image model `sd-server` is serving now, if one is; uninstalling
    /// waits for it to stop.
    pub running_model: Option<String>,
    /// The command line that installs it, from source where no pre-built
    /// build fits.
    pub install_command: String,
}

impl SetupOps {
    /// Install the pinned pre-built `sd-server`, streaming progress on `tx`
    /// as [`Self::install_llama`] does, with the same events.
    pub async fn install_sd(
        &self,
        tx: tokio::sync::mpsc::Sender<LlamaProgressEvent>,
    ) -> Result<(), GuiError> {
        gglib_runtime::sd::install_sd_prebuilt(tx)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to install stable-diffusion.cpp: {e}")))
    }

    /// What is installed, what an install would download, and what is
    /// running on it.
    pub async fn sd_status(
        &self,
        runtime: &dyn ModelRuntimePort,
    ) -> Result<ImageRuntimeStatus, GuiError> {
        let running_model = running_image_model(runtime).await;
        // Probing runs `sd-server --version`, and on Linux and Windows the
        // GPU probe runs tools too: blocking work.
        let (install, prebuilt) =
            tokio::task::spawn_blocking(|| (sd_status(), check_sd_prebuilt_availability()))
                .await
                .map_err(|e| GuiError::Internal(format!("Status task panicked: {e}")))?;
        let install = install.map_err(|e| GuiError::Internal(e.to_string()))?;
        let prebuilt = prebuilt.map(|asset| (asset.description, asset.warning));
        Ok(status_of(install, prebuilt, running_model))
    }

    /// Remove `.sd/` whole, unless an image model is running on it.
    pub async fn uninstall_sd(
        &self,
        runtime: &dyn ModelRuntimePort,
    ) -> Result<UninstallOutcome, GuiError> {
        refuse_while_drawing(runtime).await?;
        tokio::task::spawn_blocking(uninstall_sd)
            .await
            .map_err(|e| GuiError::Internal(format!("Uninstall task panicked: {e}")))?
            .map_err(|e| GuiError::Internal(e.to_string()))
    }
}

/// The status from its parts: `prebuilt` is this machine's pre-built build
/// and what to say about it, or why there is none.
pub(super) fn status_of(
    install: SdStatus,
    prebuilt: Result<(&str, Option<&str>), String>,
    running_model: Option<String>,
) -> ImageRuntimeStatus {
    let (prebuilt, prebuilt_unavailable, warning) = match prebuilt {
        Ok((description, warning)) => (
            Some(description.to_owned()),
            None,
            warning.map(str::to_owned),
        ),
        Err(reason) => (None, Some(reason), None),
    };
    ImageRuntimeStatus {
        install,
        prebuilt,
        prebuilt_unavailable,
        warning,
        running_model,
        install_command: SD_INSTALL_COMMAND.to_owned(),
    }
}

/// The name of the image model running on `sd-server`, if one is.
pub(super) async fn running_image_model(runtime: &dyn ModelRuntimePort) -> Option<String> {
    runtime
        .list_running()
        .await
        .into_iter()
        .find(|handle| handle.runtime == RuntimeKind::StableDiffusion)
        .map(|handle| handle.model_name)
}

/// A conflict while an image model runs: removing the binary and library
/// under a running server leaves it serving from deleted files and the next
/// launch failing.
pub(super) async fn refuse_while_drawing(runtime: &dyn ModelRuntimePort) -> Result<(), GuiError> {
    running_image_model(runtime).await.map_or(Ok(()), |model| {
        Err(GuiError::Conflict(format!(
            "{model} is running on sd-server. Stop it before removing the image runtime."
        )))
    })
}

#[cfg(test)]
#[path = "setup_image_runtime_tests.rs"]
mod tests;
