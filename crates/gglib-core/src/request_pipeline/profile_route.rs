//! Routing of `{model}:{profile}` request ids.
//!
//! A client selects a named sampling profile by suffixing the model it asks
//! for — `qwen3.6:coding`. This module decides, for one requested id, whether
//! that id names a model outright, a model plus a configured profile, or a
//! profile that does not exist.
//!
//! # Why the catalog decides, not a pattern
//!
//! Colons are legitimate inside model names: Ollama-style `name:tag` ids are
//! everywhere, so `qwen3.6:27b` may well *be* a model. A purely lexical rule
//! cannot tell that apart from `qwen3.6:coding` naming a profile, so this
//! module asks the catalog instead. A full-string catalog hit always wins,
//! which means adding profiles can never shadow a model that already exists.
//!
//! # Why an unmatched suffix is an error
//!
//! When the suffix matches no profile and the *base* is a real model, the
//! request is not forwarded — it fails with 404. Silently falling back to the
//! bare model is the dangerous option: a coding agent whose profile was
//! renamed or deleted would keep working while quietly sampling at the wrong
//! temperature, which is exactly the failure this feature exists to prevent. A
//! loud 404 at the moment of the rename is far cheaper to diagnose.
//!
//! That branch cannot distinguish a deleted profile from a model tag that was
//! never in the catalog (`qwen3.6:27b` with no such model *and* no `27b`
//! profile), so the error names both readings rather than guessing.
//!
//! # Cost
//!
//! An id with no `:` returns immediately with no catalog access at all, which
//! is every request from a client that does not use profiles. Only
//! colon-bearing ids reach the catalog.
//!
//! # Why this runs before the pipeline, not inside it
//!
//! [`apply`](super::apply()) is the request pipeline, and its stages all shape a
//! request that is already known to belong to some model. This does not: it
//! decides *which* model the request names, which is the question
//! [`resolve`](super::resolve()) needs answered before it can build a
//! [`ModelContext`](super::ModelContext) at all. So it sits ahead of the
//! pipeline rather than in it — a caller routes first, then resolves the base
//! name it gets back, then applies.
//!
//! It lives in `gglib-core` rather than beside the proxy that first needed it
//! because the CLI selects profiles too, and `gglib-core` is the only crate
//! both can reach.

use crate::domain::InferenceProfile;
use crate::ports::ModelCatalogPort;
use tracing::{debug, warn};

/// What a requested model id turned out to mean.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelRoute<'a> {
    /// The id names a model directly; no profile applies.
    ///
    /// Also the outcome when neither the full id nor its base resolves — the
    /// request continues to the normal model-not-found path rather than being
    /// second-guessed here.
    Bare(&'a str),

    /// The id named a model plus a configured profile.
    Profiled {
        /// The base model name, with the profile suffix removed.
        model: &'a str,
        /// The selected profile.
        profile: &'a InferenceProfile,
    },

    /// The base names a real model but the suffix matches no configured
    /// profile.
    ProfileNotFound {
        /// The full id as requested, for the error message.
        requested: &'a str,
        /// The suffix that failed to match.
        suffix: &'a str,
    },
}

/// Resolve a requested model id into a [`ModelRoute`].
///
/// Resolution order, first match wins:
///
/// 1. No `:` in the id — [`ModelRoute::Bare`], without touching the catalog.
/// 2. The full id resolves in the catalog — [`ModelRoute::Bare`]. A real model
///    whose name contains a colon always beats a profile reading.
/// 3. The suffix after the last `:` matches a configured profile —
///    [`ModelRoute::Profiled`].
/// 4. The base resolves in the catalog — [`ModelRoute::ProfileNotFound`].
/// 5. Otherwise [`ModelRoute::Bare`], leaving the existing model-not-found
///    path to report it.
///
/// Splitting on the *last* colon lets a colon-bearing model name still carry a
/// profile (`qwen:27b:coding`).
///
/// Catalog errors are treated as "not found" and logged: a degraded catalog
/// should not turn into a hard failure on a request that may not need a profile
/// at all.
pub async fn resolve_route<'a>(
    requested: &'a str,
    profiles: &'a [InferenceProfile],
    catalog: &dyn ModelCatalogPort,
) -> ModelRoute<'a> {
    // 1. The overwhelmingly common case: no profile suffix, no catalog access.
    let Some((base, suffix)) = requested.rsplit_once(':') else {
        return ModelRoute::Bare(requested);
    };

    // 2. A model that genuinely owns this name wins outright.
    if model_exists(catalog, requested).await {
        return ModelRoute::Bare(requested);
    }

    // 3. A configured profile.
    if let Some(profile) = profiles.iter().find(|p| p.name == suffix) {
        debug!(model = %base, profile = %suffix, "resolved model:profile request");
        return ModelRoute::Profiled {
            model: base,
            profile,
        };
    }

    // 4. Base is real, suffix means nothing — fail loudly rather than sample
    //    at the wrong temperature without saying so.
    if model_exists(catalog, base).await {
        warn!(
            requested = %requested,
            suffix = %suffix,
            "request names no configured profile; rejecting rather than falling back"
        );
        return ModelRoute::ProfileNotFound { requested, suffix };
    }

    // 5. Nothing matched. Let the normal model-not-found path speak.
    ModelRoute::Bare(requested)
}

/// Whether `name` resolves in the catalog, treating a query failure as absent.
async fn model_exists(catalog: &dyn ModelCatalogPort, name: &str) -> bool {
    match catalog.resolve_model(name).await {
        Ok(found) => found.is_some(),
        Err(e) => {
            warn!(model = %name, error = %e, "catalog lookup failed during profile routing");
            false
        }
    }
}

#[cfg(test)]
#[path = "profile_route_tests.rs"]
mod profile_route_tests;
