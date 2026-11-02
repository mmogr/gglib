//! The files one download of a model fetches.
//!
//! [`download_group`] is asked by everything that needs that answer: the
//! resolver that queues a download, and a repair. [`largest_projector`] is
//! the projector half of it, for a listing whose variants' roles are read.

use super::client::HfClientPort;
use super::error::HfPortResult;
use super::types::{HfFileInfo, HfQuantInfo};
use super::variant_heads::read_roles;
use super::variants::variants_of;
use crate::download::GgufFileRole;
use crate::ports::GgufParserPort;

/// What a download of one variant of a repository fetches.
#[derive(Debug, Clone)]
pub struct DownloadGroup {
    /// The weights files of the variant, in shard order.
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

/// The files a download of the variant labelled `label` from `model_id`
/// fetches.
///
/// They are the variant's weights, and the repository's projector
/// ([`repository_projector`]), read with `parser`. With no parser no head
/// can be read, and no projector is fetched.
///
/// # Errors
///
/// Whatever the client answers for either listing; a label the repository
/// has no variant of is the client's `QuantizationNotFound`.
pub async fn download_group(
    client: &dyn HfClientPort,
    parser: Option<&dyn GgufParserPort>,
    model_id: &str,
    label: &str,
) -> HfPortResult<DownloadGroup> {
    let weights = client.get_quantization_files(model_id, label).await?;
    let projector = match parser {
        Some(parser) => repository_projector(client, parser, model_id).await?,
        None => None,
    };
    Ok(DownloadGroup { weights, projector })
}

/// The projector every download of `model_id` fetches: the largest of its
/// files whose head, read with `parser`, says it is a projector. `None` when
/// none does.
///
/// Each variant's first file has its head read, four at once.
///
/// # Errors
///
/// Whatever the client answers for the repository's listing.
pub async fn repository_projector(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
) -> HfPortResult<Option<HfFileInfo>> {
    let files = client.list_gguf_files(model_id).await?;
    let variants = variants_of(files.iter().map(|file| (file.path.as_str(), file.size)));
    let variants = read_roles(client, parser, model_id, variants).await;
    let first = largest_projector(&variants).and_then(|projector| projector.file_paths.first());
    Ok(first.and_then(|path| files.into_iter().find(|file| &file.path == path)))
}

/// The projector a download fetches among `variants`, whose roles are read:
/// the largest whose header says it is a projector, the first by label of
/// two the same size.
#[must_use]
pub fn largest_projector(variants: &[HfQuantInfo]) -> Option<&HfQuantInfo> {
    variants
        .iter()
        .filter(|variant| variant.role == Some(GgufFileRole::Projector))
        .max_by(|a, b| {
            a.total_size
                .cmp(&b.total_size)
                .then_with(|| b.label.cmp(&a.label))
        })
}

#[cfg(test)]
#[path = "download_group_tests.rs"]
mod tests;
