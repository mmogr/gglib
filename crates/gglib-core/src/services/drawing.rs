//! Drawing's two rules, each asked at every door: which image model draws a
//! request, and whether a message sent with Draw pressed can draw here.
//!
//! # Which image model draws
//!
//! [`resolve_image_model`] is the one rule, read from the top; the first row
//! that applies decides:
//!
//! | | The model is | when it is unknown | when it is not an image model | when it lacks a file | otherwise |
//! |---|---|---|---|---|---|
//! | 1 | the one the request names, by id or name | refused `model_not_found` | refused `not_an_image_model` | refused `image_model_incomplete` | it draws |
//! | 2 | the settings' default image model | passed over | passed over | refused `drawing_unavailable`, naming what it lacks | it draws |
//! | 3 | the only image model with every file | | | refused `drawing_unavailable` when there is none, or several | it draws |
//!
//! A named model is never replaced: when it cannot draw the request is
//! refused with why, and neither the default nor the only complete one is
//! tried in its place. Every refusal comes before anything queues or loads.
//!
//! # Whether a message can draw
//!
//! [`drawing_availability`] is the rule behind the Draw button's
//! availability, at every door that asks (`GET /api/images/drawing`, the
//! proxy's `GET /v1/images/drawing`, and a run's `draw: true`).
//!
//! The reasons, in the order they are asked:
//!
//! 1. the chat's model is on another machine, whose loop would run its tools
//!    here while the image model is there;
//! 2. the chat's model calls no tools, so it could never call the tool;
//! 3. this process has no image driver (a proxy outside the daemon);
//! 4. what the driver answers ([`ImageGenerationPort::drawing`]): no image
//!    runtime, or what [`image_model_answer`] reads from the model rule.
//!
//! The first three, and every refusal of a request that names no model, are
//! coded `drawing_unavailable`, whatever the reason: a client greys the
//! button and shows the words. A chosen model (`image_model=<id>`) is judged
//! by row 1 of the table and refused with that row's code.
//!
//! The answer also lists every image model here
//! ([`DrawingAvailability::models`]), whether or not it can draw, so a
//! greyed button can still offer the others.

use crate::contracts::http::images::{DrawingAvailability, ImageModelRow};
use crate::domain::ImageFamily;
use crate::ports::{
    CatalogError, ImageError, ImageGenerationPort, ModelCatalogPort, ModelLaunchSpec,
    ModelRuntimeError,
};

/// The code of a refusal that is not about one chosen model.
const UNAVAILABLE: &str = "drawing_unavailable";

/// Whether a message can draw.
///
/// `far` when the chat's model is on another
/// machine, `calls_tools` `Some(false)` for a model that calls no tools,
/// `images` this process's image driver, `chosen` the id of the image model
/// a person picked (`None` for the one a request that names none draws
/// with).
pub async fn drawing_availability(
    images: Option<&dyn ImageGenerationPort>,
    far: bool,
    calls_tools: Option<bool>,
    chosen: Option<u32>,
) -> DrawingAvailability {
    if far {
        return DrawingAvailability::refused(
            UNAVAILABLE,
            "this chat's model is on another machine; a chat kept there draws with that \
             machine's image model",
        );
    }
    if calls_tools == Some(false) {
        return DrawingAvailability::refused(
            UNAVAILABLE,
            "this chat's model calls no tools, so it cannot draw; choose a model that calls tools",
        );
    }
    let Some(images) = images else {
        return DrawingAvailability::refused(
            UNAVAILABLE,
            "this proxy runs outside the gglib daemon, so it has nothing to draw with",
        );
    };
    images.drawing(chosen).await
}

/// `error` as the availability answer: the code a request to draw would be
/// refused with, and its words.
#[must_use]
pub fn drawing_refusal(error: &ImageError) -> DrawingAvailability {
    let code = match error {
        ImageError::Runtime(ModelRuntimeError::ModelNotFound(_)) => "model_not_found",
        ImageError::Runtime(ModelRuntimeError::ImageModelIncomplete { .. }) => {
            "image_model_incomplete"
        }
        other => other.code().unwrap_or(UNAVAILABLE),
    };
    DrawingAvailability::refused(code, error.to_string())
}

/// The driver's part of the answer, read from the catalogue by the model
/// rule.
///
/// Which image model draws (`chosen` judged by row 1, else rows 2 and 3 with
/// `default_model`), and a row for every image model, `resident` being the
/// ids loaded now.
///
/// A catalogue that cannot be read is a refusal with no rows.
pub async fn image_model_answer(
    catalog: &dyn ModelCatalogPort,
    default_model: Option<i64>,
    chosen: Option<u32>,
    resident: &[u32],
) -> DrawingAvailability {
    let specs = match image_models(catalog).await {
        Ok(specs) => specs,
        Err(e) => return drawing_refusal(&e),
    };
    let unnamed = match settings_default(catalog, default_model).await {
        Ok(Some(decided)) => decided,
        Ok(None) => the_only_one(specs.clone()),
        Err(e) => Err(e),
    };
    let default_id = unnamed.as_ref().ok().map(|spec| spec.id);
    let judged = match chosen {
        Some(id) => named_model(catalog, &id.to_string()).await,
        None => unnamed,
    };
    let answer = match (judged, chosen) {
        (Ok(spec), _) => DrawingAvailability {
            model_id: Some(spec.id),
            ..DrawingAvailability::with(spec.name)
        },
        // The same code as a request naming it, in words for the person who
        // chose it: the id alone would tell them nothing.
        (Err(e @ ImageError::Runtime(ModelRuntimeError::ModelNotFound(_))), Some(id)) => {
            DrawingAvailability {
                reason: Some(format!(
                    "the image model chosen (id {id}) is no longer on this machine; choose \
                     another"
                )),
                ..drawing_refusal(&e)
            }
        }
        (Err(e), _) => drawing_refusal(&e),
    };
    answer.listing(rows(&specs, default_id, resident))
}

/// A row for each of `specs` that is an image model, by family in
/// [`ImageFamily::ALL`]'s order and then by name.
fn rows(
    specs: &[ModelLaunchSpec],
    default_id: Option<u32>,
    resident: &[u32],
) -> Vec<ImageModelRow> {
    let order = |family: ImageFamily| ImageFamily::ALL.iter().position(|f| *f == family);
    let mut rows: Vec<ImageModelRow> = specs
        .iter()
        .filter_map(|spec| {
            let missing = spec.missing_components();
            Some(ImageModelRow {
                id: spec.id,
                name: spec.name.clone(),
                family: spec.image_family?,
                complete: missing.is_empty(),
                missing,
                default: default_id == Some(spec.id),
                resident: resident.contains(&spec.id),
            })
        })
        .collect();
    rows.sort_by(|a, b| (order(a.family), &a.name).cmp(&(order(b.family), &b.name)));
    rows
}

/// The image model that draws a request.
///
/// The one it names (`named`, an id or a name), else the settings' default
/// (`default_model`), else the only image model with every file its family
/// needs. The table in the [module docs](self) is this function, row by row.
///
/// # Errors
///
/// For a named model, [`ModelRuntimeError::ModelNotFound`],
/// [`ImageError::NotAnImageModel`] or
/// [`ModelRuntimeError::ImageModelIncomplete`]; with none named,
/// [`ImageError::Unavailable`] with the reason; or a catalogue failure.
pub async fn resolve_image_model(
    catalog: &dyn ModelCatalogPort,
    default_model: Option<i64>,
    named: Option<&str>,
) -> Result<ModelLaunchSpec, ImageError> {
    match named {
        Some(name) => named_model(catalog, name).await,
        None => unnamed_model(catalog, default_model).await,
    }
}

fn catalog_failed(e: CatalogError) -> ImageError {
    match e {
        CatalogError::QueryFailed(msg) | CatalogError::Internal(msg) => {
            ImageError::Runtime(ModelRuntimeError::Internal(msg))
        }
    }
}

/// Row 1: the named model, or the refusal it earns. Nothing else is tried.
async fn named_model(
    catalog: &dyn ModelCatalogPort,
    name: &str,
) -> Result<ModelLaunchSpec, ImageError> {
    let spec = catalog
        .resolve_for_launch(name)
        .await
        .map_err(catalog_failed)?
        .ok_or_else(|| ImageError::Runtime(ModelRuntimeError::ModelNotFound(name.to_owned())))?;
    if spec.image_family.is_none() {
        return Err(ImageError::NotAnImageModel { model: spec.name });
    }
    let missing = spec.missing_components();
    if !missing.is_empty() {
        return Err(ImageError::Runtime(
            ModelRuntimeError::ImageModelIncomplete {
                model: spec.name,
                missing,
            },
        ));
    }
    Ok(spec)
}

/// Rows 2 and 3: the settings' default when it is an image model, refused
/// when it lacks a file; else the only complete image model. A default that
/// is gone, or no longer an image model, is passed over as if unset.
async fn unnamed_model(
    catalog: &dyn ModelCatalogPort,
    default_model: Option<i64>,
) -> Result<ModelLaunchSpec, ImageError> {
    if let Some(decided) = settings_default(catalog, default_model).await? {
        return decided;
    }
    the_only_one(image_models(catalog).await?)
}

/// Row 2: what the settings' default decides, or `None` when it decides
/// nothing (unset, gone, or not an image model) and row 3 is asked.
async fn settings_default(
    catalog: &dyn ModelCatalogPort,
    default_model: Option<i64>,
) -> Result<Option<Result<ModelLaunchSpec, ImageError>>, ImageError> {
    let Some(id) = default_model else {
        return Ok(None);
    };
    let spec = catalog
        .resolve_for_launch(&id.to_string())
        .await
        .map_err(catalog_failed)?;
    let Some(spec) = spec.filter(|s| s.image_family.is_some()) else {
        return Ok(None);
    };
    let missing = spec.missing_components();
    if missing.is_empty() {
        return Ok(Some(Ok(spec)));
    }
    Ok(Some(Err(ImageError::Unavailable {
        reason: ModelRuntimeError::ImageModelIncomplete {
            model: spec.name,
            missing,
        }
        .to_string(),
    })))
}

/// Every image model in the catalogue, as it would launch.
async fn image_models(catalog: &dyn ModelCatalogPort) -> Result<Vec<ModelLaunchSpec>, ImageError> {
    let mut specs = Vec::new();
    for summary in catalog.list_models().await.map_err(catalog_failed)? {
        if !summary.image_output {
            continue;
        }
        let spec = catalog
            .resolve_for_launch(&summary.id.to_string())
            .await
            .map_err(catalog_failed)?;
        specs.extend(spec);
    }
    Ok(specs)
}

/// The only one of `specs` with every file its family needs, or why there is
/// not exactly one.
fn the_only_one(specs: Vec<ModelLaunchSpec>) -> Result<ModelLaunchSpec, ImageError> {
    let (mut complete, mut incomplete): (Vec<_>, Vec<_>) = specs
        .into_iter()
        .partition(|spec| spec.missing_components().is_empty());
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

#[cfg(test)]
#[path = "drawing_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "drawing_model_tests.rs"]
mod model_tests;

#[cfg(test)]
#[path = "drawing_contract_tests.rs"]
mod contract_tests;
