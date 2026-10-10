//! What a launch needs on disk: checked by `ResidentSet::admit` before the
//! request joins the queue, so a refusal displaces nothing, and again by the
//! launch once the model it displaces is stopped.

use std::path::Path;

use gglib_core::domain::RuntimeKind;
use gglib_core::ports::{ModelLaunchSpec, ModelRuntimeError};

/// Fails with the first thing `spec`'s launch would lack: its weights, then
/// its projector; and for an image model, `sd-server` itself
/// ([`ModelRuntimeError::ImageRuntimeNotInstalled`]), a file for every role
/// its family needs ([`ModelRuntimeError::ImageModelIncomplete`], naming them
/// all), and each linked file on disk, in role order.
pub(in crate::process::residency) async fn preflight(
    spec: &ModelLaunchSpec,
    sd_server: &Path,
) -> Result<(), ModelRuntimeError> {
    ensure_present(spec).await?;
    if spec.runtime() != RuntimeKind::StableDiffusion {
        return Ok(());
    }
    if !sd_server.is_file() {
        return Err(ModelRuntimeError::ImageRuntimeNotInstalled);
    }
    let missing = spec.missing_components();
    if !missing.is_empty() {
        return Err(ModelRuntimeError::ImageModelIncomplete {
            model: spec.name.clone(),
            missing,
        });
    }
    let mut components: Vec<_> = spec.components.iter().collect();
    components.sort_by_key(|c| c.role);
    for component in components {
        if !tokio::fs::try_exists(&component.path)
            .await
            .unwrap_or(false)
        {
            return Err(ModelRuntimeError::ModelFileNotFound(
                component.path.display().to_string(),
            ));
        }
    }
    Ok(())
}

/// Fails with the first of `spec`'s files that is not on disk: the weights,
/// then the projector.
///
/// A missing projector is refused here by name, as missing weights are,
/// because llama-server given a `--mmproj` path with no file exits during
/// startup and the launch would report only that its server never became
/// healthy.
pub(super) async fn ensure_present(spec: &ModelLaunchSpec) -> Result<(), ModelRuntimeError> {
    for file in std::iter::once(&spec.file_path).chain(&spec.projector) {
        if !tokio::fs::try_exists(file).await.unwrap_or(false) {
            return Err(ModelRuntimeError::ModelFileNotFound(
                file.display().to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "launch_files_tests.rs"]
mod tests;
