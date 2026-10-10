//! Whether a resident was launched the way a request now asks.
//!
//! A context size and a projector are fixed when llama-server starts, so a
//! resident launched with another of either cannot serve the request: it is
//! recycled and the next pass launches a fresh one. That is how a changed
//! projector link takes effect, at the next request and with no restart asked
//! of anyone.

use std::path::Path;

use gglib_core::domain::{ModelComponent, RuntimeKind};
use tracing::info;

use crate::process::admission::Resident;

/// What a request resolved its model's launch to: the context and projector
/// llama-server is started with, and the component files sd-server is.
#[derive(Debug, Clone, Copy)]
pub(super) struct LaunchedAs<'a> {
    /// The context size. Not read for an image model, which has none.
    pub context: u64,
    /// The projector linked now.
    pub projector: Option<&'a Path>,
    /// The component files linked now, in any order.
    pub components: &'a [ModelComponent],
}

/// A model that chats, at `(context, projector)`, with no components.
impl<'a> From<(u64, Option<&'a Path>)> for LaunchedAs<'a> {
    fn from((context, projector): (u64, Option<&'a Path>)) -> Self {
        Self {
            context,
            projector,
            components: &[],
        }
    }
}

/// Whether `resident` was launched otherwise than this request resolved to.
///
/// A llama-server resident by its context size or projector; an
/// `sd-server` resident by its set of component files alone, never by
/// context: a relinked VAE takes effect at the next request, as a relinked
/// projector does.
pub(super) fn launched_differently<'a>(
    resident: &Resident,
    request: impl Into<LaunchedAs<'a>>,
) -> bool {
    let LaunchedAs {
        context,
        projector,
        components,
    } = request.into();
    if resident.runtime == RuntimeKind::StableDiffusion {
        let differs = !same_components(&resident.components, components);
        if differs {
            info!(
                model_name = %resident.model_name,
                running_components = ?resident.components,
                requested_components = ?components,
                "resident image model was launched with other components — recycling"
            );
        }
        return differs;
    }
    if resident.context_size != context {
        info!(
            model_name = %resident.model_name,
            running_context = %resident.context_size,
            requested_context = %context,
            "resident model was launched with a different context — recycling"
        );
        return true;
    }
    if resident.projector.as_deref() != projector {
        info!(
            model_name = %resident.model_name,
            running_projector = ?resident.projector,
            requested_projector = ?projector,
            "resident model was launched with a different projector — recycling"
        );
        return true;
    }
    false
}

/// Whether two component lists name the same file for each role, whatever
/// their order.
fn same_components(running: &[ModelComponent], requested: &[ModelComponent]) -> bool {
    let sorted = |list: &[ModelComponent]| {
        let mut pairs: Vec<_> = list.iter().map(|c| (c.role, c.path.clone())).collect();
        pairs.sort();
        pairs
    };
    sorted(running) == sorted(requested)
}

#[cfg(test)]
#[path = "resident_match_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "resident_match_admit_tests.rs"]
mod admit_tests;
