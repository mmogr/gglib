//! Resolving a model's *stored* sampling configuration, with provenance.
//!
//! The live pipeline answers "what did this request end up sampling with?".
//! This answers the question the explain surfaces ask instead — "what would
//! this model sample with, and which rung supplied each value?" — with no
//! request in hand, so the top rung is empty by construction.
//!
//! # Why it is shared rather than written twice
//!
//! `gglib model explain` and the GUI's sampling panel had a copy each, and
//! they had to agree: the same ladder, followed by the same stage-5b effort
//! gate applied to the resolution rather than to a request. Two copies of a
//! rule can only ever drift into two accounts of one resolution, and the whole
//! value of an explain surface is that it describes the resolution that
//! actually runs. So the rule lives here once, and the callers keep only what
//! genuinely differs between them — one prints, the other builds a DTO.
//!
//! It sits beside [`effort_gate`](super::effort_gate) because
//! [`suppress_stored_effort`] does, and applying that gate offline is half of
//! what this function is.

use crate::domain::{
    FieldSources, InferenceConfig, InferenceProfile, ModelSamplingContext, TemplateCaps,
};

use super::effort_gate::{SuppressedEffort, suppress_stored_effort};

/// Resolve stored sampling for one model, and report where each value came
/// from and whether the template gate silently dropped an effort level.
///
/// The request rung is deliberately empty: this explains configuration, not a
/// call, so there are no per-request parameters to occupy the top of the
/// ladder.
#[must_use]
pub fn explain_stored(
    profile: Option<&InferenceProfile>,
    model_defaults: Option<&InferenceConfig>,
    global_defaults: Option<&InferenceConfig>,
    model_ctx: ModelSamplingContext,
    caps: &Option<TemplateCaps>,
) -> (InferenceConfig, FieldSources, Option<SuppressedEffort>) {
    let (mut resolved, mut sources) = InferenceConfig::default().resolve_with_profile_explained(
        profile.map(|selected| &selected.config),
        model_defaults,
        global_defaults,
        model_ctx,
    );
    let suppressed = suppress_stored_effort(&mut resolved, &mut sources, caps);
    (resolved, sources, suppressed)
}
