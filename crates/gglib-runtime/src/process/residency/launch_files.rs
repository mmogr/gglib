//! The files a launch loads, checked before anything is stopped or spawned.

use gglib_core::ports::{ModelLaunchSpec, ModelRuntimeError};

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
