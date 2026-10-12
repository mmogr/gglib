//! What a request becomes before anything queues: the image model that draws
//! it and the body `sd-server` is sent, or the refusal that the request
//! alone earns.
//!
//! The model is the one core's rule picks
//! ([`resolve_image_model`]): the one named, else the default image model
//! the settings name, else the only complete image model on this machine.
//! Its family's recipe judges the size; one to four images.
//!
//! A request with a picture to draw from ([`Source`]) is drawn at the
//! picture's size, which the same rule judges: a picture one family drew is
//! refused by a family that does not draw that size, never resized, because
//! a result of another size cannot be pasted back. `sd-server` shortens a
//! schedule by the strength, and sampled `floor(steps x strength)` steps in
//! each of nine renders read (log-0018, 2026-10-12), so the steps asked for
//! are the recipe's divided by the strength, rounded up ([`steps_for`]):
//! a redraw then samples about as many steps as a draw, not a fraction.

use gglib_core::ports::{
    ImageError, ImageRequest, ImageSize, MAX_IMAGES_PER_REQUEST, ModelCatalogPort,
};
use gglib_core::services::resolve_image_model;

use super::job_api::{ImgGenBody, SampleParams};
use super::source::{Source, header_size};

/// A request that may queue.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Plan {
    /// The image model's id, which the admission is asked for by.
    pub(crate) model_id: u32,
    /// Its name, for the batch.
    pub(crate) model_name: String,
    /// How many images: the passes the job samples.
    pub(crate) n: u8,
    /// The render, as `sd-server` reads it.
    pub(crate) body: ImgGenBody,
    /// The picture it draws from, which each result is pasted against.
    pub(crate) source: Option<Source>,
}

/// The steps to ask `sd-server` for so that a redraw at `strength` samples
/// about `recipe_steps`: `recipe_steps / strength`, rounded up. A strength
/// that is not a number above 0 asks for the recipe's own.
pub(crate) fn steps_for(recipe_steps: u32, strength: f32) -> u32 {
    if !(strength > 0.0 && strength.is_finite()) {
        return recipe_steps;
    }
    let enough = |steps: &u32| f64::from(*steps) * f64::from(strength) >= f64::from(recipe_steps);
    (recipe_steps..=u32::MAX)
        .find(enough)
        .unwrap_or(recipe_steps)
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
    let recipe = family.recipe();
    let rule = recipe.size;
    if request.source.is_some() && request.size.is_some() {
        return Err(ImageError::Invalid {
            message: "a request with a picture is drawn at the picture's size; leave size out"
                .to_owned(),
        });
    }
    // The picture's size is read from its header and judged before the
    // picture is decoded, so nothing larger than the family draws ever is.
    let picture = request
        .source
        .as_ref()
        .map(|source| header_size(&source.image, "picture"))
        .transpose()?;
    if let Some((width, height)) = picture
        && !rule.check(width, height)
    {
        return Err(ImageError::Invalid {
            message: format!(
                "the picture is {width}x{height}, which {} does not draw ({rule}); draw from \
                 it with the model that drew it, or resize it first",
                spec.name
            ),
        });
    }
    let source = request.source.as_ref().map(Source::read).transpose()?;
    let size = picture.map_or_else(
        || {
            request.size.unwrap_or(ImageSize {
                width: rule.default_side,
                height: rule.default_side,
            })
        },
        |(width, height)| ImageSize { width, height },
    );
    if !rule.check(size.width, size.height) {
        return Err(ImageError::InvalidSize {
            width: size.width,
            height: size.height,
            rule,
        });
    }
    let (init_image, mask_image) = match &source {
        Some(source) => {
            let (picture, mask) = source.wire()?;
            (Some(picture), mask)
        }
        None => (None, None),
    };
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
            init_image,
            mask_image,
            strength: source.as_ref().map(|s| s.strength),
            sample_params: source.as_ref().map(|s| SampleParams {
                sample_steps: steps_for(recipe.steps, s.strength),
            }),
        },
        source,
    })
}
