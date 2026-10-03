//! The files one download of a model fetches.
//!
//! [`download_group`] is asked by everything that needs that answer: the
//! resolver that queues a download, and a repair.
//! [`projector_fetched_with`] is the projector half of it, for a listing that
//! already holds the repository's projectors.

use super::client::HfClientPort;
use super::error::HfPortResult;
use super::types::HfFileInfo;
use crate::download::{Quantization, choose_projector};

/// What a download of one quantization of a repository fetches.
#[derive(Debug, Clone)]
pub struct DownloadGroup {
    /// The weights files of the quantization, in shard order.
    pub weights: Vec<HfFileInfo>,
    /// The projector fetched with them, when the repository has one.
    pub projector: Option<HfFileInfo>,
}

impl DownloadGroup {
    /// Every file of the group: the weights, then the projector.
    pub fn files(&self) -> impl Iterator<Item = &HfFileInfo> {
        self.weights.iter().chain(&self.projector)
    }
}

/// The files a download of `quantization` from `model_id` fetches: the
/// quantization's weights, and the projector [`choose_projector`] picks among
/// the repository's.
///
/// # Errors
///
/// Whatever the client answers for either listing; a quantization the
/// repository does not have is the client's `QuantizationNotFound`.
pub async fn download_group(
    client: &dyn HfClientPort,
    model_id: &str,
    quantization: Quantization,
) -> HfPortResult<DownloadGroup> {
    let weights = client
        .get_quantization_files(model_id, &quantization.to_string())
        .await?;
    let projectors = client.list_projectors(model_id).await?;
    Ok(DownloadGroup {
        weights,
        projector: projector_fetched_with(quantization, &projectors).cloned(),
    })
}

/// The projector a download of `quantization` fetches, among `projectors`,
/// the repository's: the one [`choose_projector`] picks.
///
/// A listing that says which projector comes with a quantization asks this,
/// so it names the file the download fetches.
#[must_use]
pub fn projector_fetched_with(
    quantization: Quantization,
    projectors: &[HfFileInfo],
) -> Option<&HfFileInfo> {
    let path = choose_projector(quantization, projectors.iter().map(|p| p.path.as_str()))?;
    projectors.iter().find(|p| p.path == path)
}

#[cfg(test)]
#[path = "download_group_tests.rs"]
mod tests;
