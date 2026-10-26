//! Either engine as the daemon installs, reports and removes it: the web
//! face of `gglib config llama|sd install|status|uninstall`. A `#[path]`
//! child of `setup.rs`, so it adds to [`SetupOps`].
//!
//! Only the download is offered here: a source build streams a different
//! event type and is the command line's, which these refusals name. Nothing
//! is installed or removed while a model runs on that engine.

use serde::Serialize;

use gglib_core::domain::RuntimeKind;
use gglib_core::paths::{LLAMA_INSTALL_COMMAND, SD_INSTALL_COMMAND};
use gglib_core::ports::ModelRuntimePort;
use gglib_runtime::engine::{InstallPlan, PinStanding, PinState, pin_state, plan_install};
use gglib_runtime::llama::{LlamaProgressEvent, LlamaStatus, llama_status};
use gglib_runtime::sd::{SdStatus, sd_status};

use super::SetupOps;
use crate::error::GuiError;

/// What Settings says about one engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// Which engine this is.
    pub kind: RuntimeKind,
    /// Whether its server binary is there.
    pub installed: bool,
    /// Why the installed binary does not work, when gglib checked and it does
    /// not (llama.cpp's validation); null otherwise.
    pub broken: Option<String>,
    pub binary_path: String,
    /// How it got there, from the install's record; null with no readable
    /// record.
    pub install: Option<EngineInstall>,
    /// Set when a record exists but could not be read.
    pub record_error: Option<String>,
    /// What the binary says it is when asked: llama.cpp's build (`b11429`),
    /// sd-server's `--version` line. Null when it was not asked or said
    /// nothing.
    pub reports: Option<String>,
    /// Where the install stands against the pinned release.
    pub pin: PinState,
    /// The download an install takes on this machine (`macOS ARM64 (Metal)`);
    /// null when there is none.
    pub download: Option<String>,
    /// Why there is no download, when there is none: the command line builds
    /// from source.
    pub download_unavailable: Option<String>,
    /// What to say about that download first: a CPU build's slowness.
    pub warning: Option<String>,
    /// The model running on this engine's server, if one is: nothing is
    /// installed or removed under it.
    pub serving: Option<String>,
    /// The command that installs it from a terminal.
    pub install_command: String,
    /// The command that updates it from a terminal, which a source build
    /// needs.
    pub update_command: String,
}

/// How an engine's install got there, as its record says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct EngineInstall {
    /// Built from source here, rather than downloaded.
    pub built: bool,
    /// The release it is: a tag, or a source build's short commit.
    pub release: String,
    /// The download's platform build (`macOS ARM64 (Metal)`), or the
    /// acceleration a source build compiled in (`Metal`).
    pub platform: String,
    /// When it was installed or built, as RFC 3339.
    pub date: String,
}

impl SetupOps {
    /// Where `kind` stands: what is installed and how, against its pin, what
    /// an install would download, and what runs on it.
    pub async fn engine_status(
        &self,
        runtime: &dyn ModelRuntimePort,
        kind: RuntimeKind,
    ) -> Result<EngineStatus, GuiError> {
        let serving = running_model(runtime, kind).await;
        // Probing runs the binary's `--version`, and on Linux and Windows the
        // GPU probe runs tools too: blocking work.
        let status = tokio::task::spawn_blocking(move || -> anyhow::Result<EngineStatus> {
            let download = gglib_runtime::engine::download(kind);
            Ok(match kind {
                RuntimeKind::Llama => llama_engine_status(llama_status()?, download),
                RuntimeKind::StableDiffusion => sd_engine_status(sd_status()?, download),
            })
        })
        .await
        .map_err(|e| GuiError::Internal(format!("Status task panicked: {e}")))?
        .map_err(|e| GuiError::Internal(e.to_string()))?;
        Ok(EngineStatus { serving, ..status })
    }

    /// Install `kind`'s pinned download, streaming progress on `tx`, unless
    /// a model runs on it or the install would be a source build.
    pub async fn install(
        &self,
        runtime: &dyn ModelRuntimePort,
        kind: RuntimeKind,
        tx: tokio::sync::mpsc::Sender<LlamaProgressEvent>,
    ) -> Result<(), GuiError> {
        refuse_while_serving(runtime, kind, "installing").await?;
        refuse_a_source_build(kind)?;
        match kind {
            RuntimeKind::Llama => gglib_runtime::llama::download_prebuilt_binaries(tx).await,
            RuntimeKind::StableDiffusion => gglib_runtime::sd::install_sd_prebuilt(tx).await,
        }
        .map_err(|e| GuiError::Internal(format!("Failed to install {}: {e}", kind.label())))
    }
}

/// The commands that install and update `kind` from a terminal.
const fn commands(kind: RuntimeKind) -> (&'static str, &'static str) {
    match kind {
        RuntimeKind::Llama => (LLAMA_INSTALL_COMMAND, "gglib config llama update"),
        RuntimeKind::StableDiffusion => (SD_INSTALL_COMMAND, "gglib config sd update"),
    }
}

/// An engine's status with what every engine has filled in: `download` is
/// this machine's download and what to say about it, or why there is none.
fn common_status(
    kind: RuntimeKind,
    installed: bool,
    binary_path: String,
    pin: PinState,
    download: Result<(String, Option<String>), String>,
) -> EngineStatus {
    let (download, download_unavailable, warning) = match download {
        Ok((description, warning)) => (Some(description), None, warning),
        Err(reason) => (None, Some(reason), None),
    };
    let (install_command, update_command) = commands(kind);
    EngineStatus {
        kind,
        installed,
        broken: None,
        binary_path,
        install: None,
        record_error: None,
        reports: None,
        pin,
        download,
        download_unavailable,
        warning,
        serving: None,
        install_command: install_command.to_owned(),
        update_command: update_command.to_owned(),
    }
}

/// llama.cpp's, from what `gglib config llama status` reports.
pub(super) fn llama_engine_status(
    status: LlamaStatus,
    download: Result<(String, Option<String>), String>,
) -> EngineStatus {
    let install = match (status.build, status.prebuilt) {
        (Some(build), _) => Some(EngineInstall {
            built: true,
            release: build.version,
            platform: build.acceleration,
            date: build.build_date,
        }),
        (None, Some(prebuilt)) => Some(EngineInstall {
            built: false,
            release: prebuilt.version,
            platform: prebuilt.platform,
            date: prebuilt.installed_at,
        }),
        (None, None) => None,
    };
    EngineStatus {
        broken: status.health_error,
        install,
        record_error: status.build_error,
        reports: status.runtime.map(|runtime| runtime.build_label()),
        ..common_status(
            RuntimeKind::Llama,
            status.installed,
            status.binary_path,
            status.pin,
            download,
        )
    }
}

/// stable-diffusion.cpp's, from what `gglib config sd status` reports.
pub(super) fn sd_engine_status(
    status: SdStatus,
    download: Result<(String, Option<String>), String>,
) -> EngineStatus {
    let install = match (status.release, status.platform, status.installed_at) {
        (Some(release), Some(platform), Some(date)) => Some(EngineInstall {
            built: status.install_type.as_deref() == Some("source"),
            release,
            platform,
            date,
        }),
        _ => None,
    };
    EngineStatus {
        install,
        record_error: status.record_error,
        reports: status.version_line,
        ..common_status(
            RuntimeKind::StableDiffusion,
            status.installed,
            status.binary_path,
            status.pin,
            download,
        )
    }
}

/// The name of the model running on `kind`'s server, if one is.
pub(crate) async fn running_model(
    runtime: &dyn ModelRuntimePort,
    kind: RuntimeKind,
) -> Option<String> {
    runtime
        .list_running()
        .await
        .into_iter()
        .find(|handle| handle.runtime == kind)
        .map(|handle| handle.model_name)
}

/// A conflict when an install of `kind` would be a source build, which is
/// the command line's to run: a source build of a release is there, a GPU
/// build would otherwise become a CPU one, or this machine has no download.
/// The daemon installs downloads only.
pub(crate) fn refuse_a_source_build(kind: RuntimeKind) -> Result<(), GuiError> {
    let state = pin_state(kind).map_err(|e| GuiError::Internal(e.to_string()))?;
    let InstallPlan::Build { no_download } = plan_install(kind, &state, false, false) else {
        return Ok(());
    };
    let (_, command) = commands(kind);
    let product = kind.label();
    Err(GuiError::Conflict(match no_download {
        Some(reason) => format!(
            "{}. Build {product} from a terminal: run '{command}'.",
            reason.trim_end_matches('.')
        ),
        // A record of a source build with nothing installed: the binary was
        // removed by hand, and the command line builds it again.
        None if state.standing == PinStanding::Missing => format!(
            "{product} was built from source here before, so it is built again from a terminal: \
             run '{command}'."
        ),
        None => format!(
            "{product} was built from source here, so it is updated from a terminal, which \
             builds it again: run '{command}'."
        ),
    }))
}

/// A conflict while a model runs on `kind`'s server: replacing or removing
/// the binary and its library under a running server leaves it serving from
/// deleted files and the next launch failing. `doing` is what was refused,
/// `installing` or `removing`.
pub(crate) async fn refuse_while_serving(
    runtime: &dyn ModelRuntimePort,
    kind: RuntimeKind,
    doing: &str,
) -> Result<(), GuiError> {
    running_model(runtime, kind).await.map_or(Ok(()), |model| {
        Err(GuiError::Conflict(format!(
            "{model} is running on {}. Stop it before {doing} {}.",
            kind.server_name(),
            kind.label()
        )))
    })
}

#[cfg(test)]
#[path = "setup_engines_tests.rs"]
mod tests;
