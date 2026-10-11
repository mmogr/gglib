//! What a request becomes before anything queues: the image model that draws
//! it and the body `sd-server` is sent, or the refusal that the request
//! alone earns.
//!
//! The model is the one core's rule picks
//! ([`resolve_image_model`]): the one named, else the default image model
//! the settings name, else the only complete image model on this machine.
//! Its family's recipe judges the size; one to four images.

use gglib_core::ports::{
    ImageError, ImageRequest, ImageSize, MAX_IMAGES_PER_REQUEST, ModelCatalogPort,
};
use gglib_core::services::resolve_image_model;

use super::job_api::ImgGenBody;

/// A request that may queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The image model's id, which the admission is asked for by.
    pub(crate) model_id: u32,
    /// Its name, for the batch.
    pub(crate) model_name: String,
    /// How many images: the passes the job samples.
    pub(crate) n: u8,
    /// The render, as `sd-server` reads it.
    pub(crate) body: ImgGenBody,
}

/// Read `request` against the catalogue: [`Plan`] or its refusal.
/// `default_model` is the settings' default image model, the model a request
/// that names none draws with.
pub(crate) async fn plan(
    catalog: &dyn ModelCatalogPort,
    default_model: Option<i64>,
    request: &ImageRequest,
) -> Result<Plan, ImageError> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err(ImageError::Invalid {
            message: "a prompt is needed: say what to draw".to_owned(),
        });
    }
    if !(1..=MAX_IMAGES_PER_REQUEST).contains(&request.n) {
        return Err(ImageError::Invalid {
            message: format!(
                "a request draws from 1 to {MAX_IMAGES_PER_REQUEST} images, not {}",
                request.n
            ),
        });
    }
    let spec = resolve_image_model(catalog, default_model, request.model.as_deref()).await?;
    let Some(family) = spec.image_family else {
        return Err(ImageError::NotAnImageModel { model: spec.name });
    };
    let rule = family.recipe().size;
    let size = request.size.unwrap_or(ImageSize {
        width: rule.default_side,
        height: rule.default_side,
    });
    if !rule.check(size.width, size.height) {
        return Err(ImageError::InvalidSize {
            width: size.width,
            height: size.height,
            rule,
        });
    }
    Ok(Plan {
        model_id: spec.id,
        model_name: spec.name,
        n: request.n,
        body: ImgGenBody {
            prompt: prompt.to_owned(),
            width: size.width,
            height: size.height,
            seed: request.seed.unwrap_or(-1),
            batch_count: request.n,
            output_format: "png",
            preview: "proj",
            preview_interval: 1,
        },
    })
}
