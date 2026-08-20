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
    let unsendable = non_finite_fields(named);

    sources
        .iter()
        .filter(|(field, source)| {
            // A non-finite value the caller named never reaches the wire at
            // all, whichever rung "won" — JSON has no NaN or infinity, so
            // `to_openai_json_patch` drops it. Reported unconditionally,
            // because the alternative is the exact silence this whole helper
            // exists to break, and because it is absent from `requested` and
            // so would otherwise fall out of the membership test below.
            if unsendable.contains(field) {
                return true;
            }
            requested.contains_key(*field)
                && !won(*source, rung)
                && requested.get(*field) != survived.get(*field)
        })
        .map(|(field, _)| field)
        .collect()
}

/// Float fields the caller set to a value JSON cannot carry.
///
/// Listed explicitly rather than derived, because the derivation would have to
/// go through the very serialisation that loses them. The drift risk that
/// creates is covered by `every_float_field_is_checked_for_non_finiteness`,
/// which fails if a new float field is added and not named here.
fn non_finite_fields(c: &InferenceConfig) -> Vec<&'static str> {
    [
        ("temperature", c.temperature),
        ("top_p", c.top_p),
        ("presence_penalty", c.presence_penalty),
        ("repeat_penalty", c.repeat_penalty),
        ("min_p", c.min_p),
        ("frequency_penalty", c.frequency_penalty),
        ("dynatemp_range", c.dynatemp_range),
        ("dynatemp_exponent", c.dynatemp_exponent),
        ("top_n_sigma", c.top_n_sigma),
        ("dry_multiplier", c.dry_multiplier),
        ("dry_base", c.dry_base),
    ]
    .into_iter()
    .filter_map(|(field, value)| match value {
        Some(v) if !v.is_finite() => Some(field),
        _ => None,
    })
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

#[cfg(test)]
#[path = "sampling_discards_non_finite_tests.rs"]
mod sampling_discards_non_finite_tests;
