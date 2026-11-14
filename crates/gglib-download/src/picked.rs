//! The variant `gglib model download <repo>` takes with none asked, for a
//! listing to mark before anything is downloaded.

use std::sync::Arc;

use gglib_core::download::DownloadError;
use gglib_core::ports::{GgufParserPort, HfClientPort, QuantizationResolver};

use crate::cached_hub::CachedHub;
use crate::quant_selector::pick_among;
use crate::resolver::HfQuantizationResolver;
use crate::variant_reader::HubSource;

/// The label of the variant a download of `repo_id` takes when no
/// quantization is asked for, against `budget_bytes`; `None` when the
/// repository holds no model.
///
/// The download's own choice, made as a download makes it: the repository
/// listed and each variant's role read from its head, then each head read
/// as the pick walks, and a header a head stops short of read further in
/// steps, all through one cache (`QuantizationSelector`'s `pick_among`).
/// It costs those reads, seconds and megabytes on a large repository, so a
/// listing asks it apart from the listing.
///
/// # Errors
///
/// What listing the repository answers when it fails.
pub async fn download_pick(
    client: Arc<dyn HfClientPort>,
    parser: Arc<dyn GgufParserPort>,
    repo_id: &str,
    budget_bytes: Option<u64>,
) -> Result<Option<String>, DownloadError> {
    let hub: Arc<dyn HfClientPort> = Arc::new(CachedHub::new(client));
    let resolver = HfQuantizationResolver::new(Arc::clone(&hub), Arc::clone(&parser));
    let available = resolver.list_available(repo_id).await?;
    let source = HubSource {
        client: hub,
        parser,
    };
    Ok(pick_among(&source, repo_id, &available, budget_bytes)
        .await
        .ok()
        .map(|pick| available[pick.index].label.clone()))
}
