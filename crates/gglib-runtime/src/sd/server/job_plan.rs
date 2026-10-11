//! What a request becomes before anything queues: the image model that draws
//! it and the body `sd-server` is sent, or the refusal that the request
//! alone earns.
//!
//! The model is the one named, else the default image model the settings
//! name, else the only complete image model on this machine
//! ([`drawing_model`]). Its family's recipe judges the size; one to four
//! images.

use gglib_core::ports::{
    CatalogError, ImageError, ImageRequest, ImageSize, MAX_IMAGES_PER_REQUEST, ModelCatalogPort,
    ModelLaunchSpec, ModelRuntimeError,
};

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
    let spec = match &request.model {
        Some(name) => named(catalog, name).await?,
        None => drawing_model(catalog, default_model).await?,
    };
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

fn catalog_failed(e: CatalogError) -> ImageError {
    match e {
        CatalogError::QueryFailed(msg) | CatalogError::Internal(msg) => {
            ImageError::Runtime(ModelRuntimeError::Internal(msg))
        }
    }
}

async fn named(catalog: &dyn ModelCatalogPort, name: &str) -> Result<ModelLaunchSpec, ImageError> {
    catalog
        .resolve_for_launch(name)
        .await
        .map_err(catalog_failed)?
        .ok_or_else(|| ImageError::Runtime(ModelRuntimeError::ModelNotFound(name.to_owned())))
}

/// The image model a request that names none draws with, or why there is
/// none: the settings' default (`default_model`) when it is an image model,
/// refused when it lacks a file; else the only image model with every file
/// its family needs. A default that is gone, or no longer an image model,
/// is passed over as if unset.
///
/// # Errors
///
/// [`ImageError::Unavailable`] with the reason, or a catalogue failure.
pub(crate) async fn drawing_model(
    catalog: &dyn ModelCatalogPort,
    default_model: Option<i64>,
) -> Result<ModelLaunchSpec, ImageError> {
    if let Some(id) = default_model {
        let spec = catalog
            .resolve_for_launch(&id.to_string())
            .await
            .map_err(catalog_failed)?;
        if let Some(spec) = spec.filter(|s| s.image_family.is_some()) {
            let missing = spec.missing_components();
            if missing.is_empty() {
                return Ok(spec);
            }
            return Err(ImageError::Unavailable {
                reason: ModelRuntimeError::ImageModelIncomplete {
                    model: spec.name,
                    missing,
                }
                .to_string(),
            });
        }
    }
    the_only_one(catalog).await
}

/// The only image model with every file its family needs, or why there is
/// not exactly one.
async fn the_only_one(catalog: &dyn ModelCatalogPort) -> Result<ModelLaunchSpec, ImageError> {
    let mut complete = Vec::new();
    let mut incomplete = Vec::new();
    for summary in catalog.list_models().await.map_err(catalog_failed)? {
        if !summary.image_output {
            continue;
        }
        let Some(spec) = catalog
            .resolve_for_launch(&summary.id.to_string())
            .await
            .map_err(catalog_failed)?
        else {
            continue;
        };
        if spec.missing_components().is_empty() {
            complete.push(spec);
        } else {
            incomplete.push(spec);
        }
    }
    let unavailable = |reason: String| Err(ImageError::Unavailable { reason });
    match (complete.len(), incomplete.len()) {
        (1, _) => Ok(complete.remove(0)),
        (0, 0) => unavailable(
            "there is no image model on this machine to draw with; find one with `gglib model \
             search --images <words>` and download it"
                .to_owned(),
        ),
        (0, 1) => {
            let spec = incomplete.remove(0);
            let missing = spec.missing_components();
            unavailable(
                ModelRuntimeError::ImageModelIncomplete {
                    model: spec.name,
                    missing,
                }
                .to_string(),
            )
        }
        (0, _) => unavailable(format!(
            "none of this machine's image models ({}) has every file it needs; link the \
             missing ones with `gglib model update <model> --component <role>=<path>`",
            names(&incomplete)
        )),
        (_, _) => unavailable(format!(
            "this machine has {} image models that can draw ({}); name the one to draw with, \
             or set a default with `gglib config settings set --default-image-model <name>`",
            complete.len(),
            names(&complete)
        )),
    }
}

fn names(specs: &[ModelLaunchSpec]) -> String {
    specs
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
