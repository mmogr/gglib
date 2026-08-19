//! Which parameters a caller named and the ladder then passed over.
//!
//! # The gap this closes
//!
//! `presence_penalty`, `repeat_penalty` and `min_p` travel with whichever rung
//! claims `temperature` — see [`InferenceConfig::resolve_layers_with_sources`].
//! So `--profile chat --presence-penalty 1.2` with no `--temperature` silently
//! discards the penalty: the profile claimed the temperature, and the coupled
//! trio comes only from the rung that did.
//!
//! That is correct, and it is invisible. A resolved config cannot show it —
//! a discarded value and a value nobody ever named look identical once the
//! ladder has folded. Only the provenance distinguishes them.
//!
//! # Why the rung is a parameter
//!
//! There is more than one ladder. The stored ladder
//! ([`InferenceConfig::resolve_with_profile`]) puts the request at rung 0; the
//! request pipeline's puts `cli` at 0 and the *client* at 1. A helper that
//! hardcoded 0 would be right on one and quietly wrong on the other, reporting
//! the client's losses as the operator's.
//!
//! # Why `FloorCoupled` alone is not the test
//!
//! [`ParamSource::FloorCoupled`] says the coupling rule fired — not that *this*
//! caller lost. When the claiming rung supplied its own value the field reports
//! [`ParamSource::Layer`] instead, and the caller's value is just as gone. The
//! test is therefore "did my rung win", not "which rule ran".

use crate::domain::{FieldSources, InferenceConfig, ParamSource};

/// The parameters `named` set that did **not** survive to the resolution.
///
/// Compares values, not just provenance: when the winning rung happens to name
/// the same number, nothing was lost and there is nothing to report.
///
/// Returns `snake_case` field names, matching [`FieldSources::iter`] and
/// [`InferenceConfig::to_openai_json_patch`].
#[must_use]
pub fn discarded_from_rung(
    named: &InferenceConfig,
    resolved: &InferenceConfig,
    sources: &FieldSources,
    rung: usize,
) -> Vec<&'static str> {
    let requested = named.to_openai_json_patch();
    let survived = resolved.to_openai_json_patch();

    sources
        .iter()
        .filter(|(field, _)| requested.contains_key(*field))
        .filter(|(field, source)| {
            !won(*source, rung) && requested.get(*field) != survived.get(*field)
        })
        .map(|(field, _)| field)
        .collect()
}

/// Whether the value at `rung` is the one that reached the resolution.
///
/// Exhaustive rather than a `matches!`, per the argument on
/// [`ParamSource::is_deliberate_choice`] and because
/// `scripts/check_param_source_exhaustive.sh` requires it: a new variant
/// should force a decision here rather than default into "not ours".
const fn won(source: ParamSource, rung: usize) -> bool {
    match source {
        // The only outcome in which this caller's value is what runs.
        ParamSource::Layer(index) => index == rung,
        // Everything else means this caller's value is not what runs:
        // `FloorCoupled` — the coupling rule passed every lower rung over,
        // this one included; `SuppressedByTemplate` — a later stage dropped it
        // because the template never reads the field; `Floor`/`Unset` — nobody
        // named it, so this caller did not either. Listed rather than
        // wildcarded so a new variant forces a decision here.
        ParamSource::FloorCoupled
        | ParamSource::SuppressedByTemplate
        | ParamSource::Floor
        | ParamSource::Unset => false,
    }
}

#[cfg(test)]
#[path = "sampling_discards_tests.rs"]
mod sampling_discards_tests;
