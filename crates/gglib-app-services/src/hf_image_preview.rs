//! What the `HuggingFace` browser says a download of an image model brings
//! beside its weights, before anything is downloaded.

use std::path::Path;

use gglib_core::download::Quantization;
use gglib_core::paths::repository_dir;
use gglib_core::ports::huggingface::image_companions;
use gglib_core::ports::{GgufParserPort, HfClientPort, HfFileInfo, HfQuantInfo};

use crate::types::{HfCompanion, HfImagePreview};

/// The quantization whose first file is read to know the family: `Q8_0`
/// when the repository has it, as an image repository's download defaults
/// to, and otherwise the smallest.
fn sniffed_quantization(quants: &[HfQuantInfo]) -> Option<&HfQuantInfo> {
    quants
        .iter()
        .find(|q| Quantization::from_filename(&q.name) == Quantization::Q8_0)
        .or_else(|| quants.iter().min_by_key(|q| q.total_size))
}

/// The image model `model_id`'s family and companions, from one head read of
/// one quantization's first file, with each companion marked present when
/// it is already in its repository's folder under `models_dir`; `None` for a
/// repository whose weights are not an image model's.
///
/// A family whose companions cannot all be looked up is logged and answered
/// as `None`: the listing of the quantizations still shows, and a download
/// stops on the same lookup with its own message.
pub(crate) async fn image_preview(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    quants: &[HfQuantInfo],
    models_dir: Option<&Path>,
) -> Option<HfImagePreview> {
    let quant = sniffed_quantization(quants)?;
    let first = HfFileInfo {
        path: quant.file_paths.first()?.clone(),
        size: quant.total_size,
        is_gguf: true,
        oid: None,
    };
    let (family, companions) = match image_companions(client, parser, model_id, &first).await {
        Ok(found) => found?,
        Err(unlisted) => {
            tracing::warn!(
                model_id,
                error = %unlisted,
                "an image model's companions could not be looked up for its preview"
            );
            return None;
        }
    };
    let companions: Vec<HfCompanion> = companions
        .into_iter()
        .map(|companion| HfCompanion {
            present: models_dir.is_some_and(|dir| {
                repository_dir(dir, &companion.repo)
                    .join(&companion.file.path)
                    .exists()
            }),
            role: companion.role,
            repo: companion.repo,
            file_path: companion.file.path,
            size_bytes: companion.file.size,
        })
        .collect();
    let fetch_bytes = companions
        .iter()
        .filter(|companion| !companion.present)
        .map(|companion| companion.size_bytes)
        .sum();
    Some(HfImagePreview {
        family,
        companions,
        fetch_bytes,
    })
}

#[cfg(test)]
#[path = "hf_image_preview_tests.rs"]
mod tests;
