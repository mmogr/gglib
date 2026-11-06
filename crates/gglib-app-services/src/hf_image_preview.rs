//! What the `HuggingFace` browser says a download of an image model brings
//! beside its weights, before anything is downloaded.

use std::path::Path;

use gglib_core::domain::ImageFamily;
use gglib_core::download::GgufFileRole;
use gglib_core::paths::repository_dir;
use gglib_core::ports::huggingface::{companions_of, image_family_of, match_variant};
use gglib_core::ports::{GgufParserPort, HfClientPort, HfFileInfo, HfQuantInfo};

use crate::types::{HfCompanion, HfImagePreview};

/// The model variant whose first file is read to know the family: the one
/// `Q8_0` names when the repository has it, as an image repository's
/// download defaults to, and otherwise the smallest. A draft, a helper or a
/// projector is never read for it.
fn sniffed_quantization(quants: &[HfQuantInfo]) -> Option<&HfQuantInfo> {
    let models: Vec<&HfQuantInfo> = quants
        .iter()
        .filter(|q| q.role.is_none_or(GgufFileRole::is_weights))
        .collect();
    match_variant(models.iter().copied(), "Q8_0")
        .ok()
        .or_else(|| models.into_iter().min_by_key(|q| q.total_size))
}

/// What a listing learns of an image repository from one head read of one
/// quantization's first file: the family, and the preview of its
/// companions when they can all be looked up.
///
/// The family stands on its own: a companion the Hub cannot find leaves the
/// preview `None` and the family known, so a variant is still judged as an
/// image model's. A repository whose weights are not an image model's has
/// neither.
pub(crate) struct ImageLook {
    pub family: Option<ImageFamily>,
    pub preview: Option<HfImagePreview>,
}

/// The image model `model_id`'s family and the preview of its companions,
/// each marked present when it is already in its repository's folder under
/// `models_dir`.
///
/// A family whose companions cannot all be looked up is logged and its
/// preview is `None`: the listing of the quantizations still shows, and a
/// download stops on the same lookup with its own message.
pub(crate) async fn image_look(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    quants: &[HfQuantInfo],
    models_dir: Option<&Path>,
) -> ImageLook {
    let none = ImageLook {
        family: None,
        preview: None,
    };
    let Some(quant) = sniffed_quantization(quants) else {
        return none;
    };
    let Some(path) = quant.file_paths.first() else {
        return none;
    };
    let first = HfFileInfo {
        path: path.clone(),
        size: quant.total_size,
        is_gguf: true,
        oid: None,
    };
    let Some(family) = image_family_of(client, parser, model_id, &first).await else {
        return none;
    };
    let preview = match companions_of(client, family).await {
        Ok(companions) => Some(preview_of(family, companions, models_dir)),
        Err(unlisted) => {
            tracing::warn!(
                model_id,
                error = %unlisted,
                "an image model's companions could not be looked up for its preview"
            );
            None
        }
    };
    ImageLook {
        family: Some(family),
        preview,
    }
}

/// The preview alone: [`image_look`]'s, `None` when the repository is not
/// an image model's or a companion could not be looked up.
#[cfg(test)]
pub(crate) async fn image_preview(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    quants: &[HfQuantInfo],
    models_dir: Option<&Path>,
) -> Option<HfImagePreview> {
    image_look(client, parser, model_id, quants, models_dir)
        .await
        .preview
}

/// `family`'s preview: its `companions` as the Hub listed them, each marked
/// present when it is already under `models_dir`, and the bytes a download
/// still fetches.
fn preview_of(
    family: ImageFamily,
    companions: Vec<gglib_core::ports::huggingface::Companion>,
    models_dir: Option<&Path>,
) -> HfImagePreview {
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
    HfImagePreview {
        family,
        companions,
        fetch_bytes,
    }
}

#[cfg(test)]
#[path = "hf_image_preview_tests.rs"]
mod tests;
