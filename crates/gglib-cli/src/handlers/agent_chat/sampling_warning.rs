//! Telling the user when a sampling flag did not survive the ladder.
//!
//! Separate from `config` because it is presentation: `config` composes an
//! agent session, and a module that both wires ports and formats stderr is two
//! things. It also kept `config.rs` under the 300 LOC budget.

use gglib_core::domain::InferenceConfig;

/// Say so when the ladder passed over a sampling flag the caller passed.
///
/// The case that motivates it: `--profile chat --presence-penalty 1.2` with no
/// `--temperature`. The profile claims the temperature, so the coupled trio
/// comes only from the profile and the penalty is silently gone — see
/// [`gglib_core::domain::discarded_from_rung`]. It predates profiles, though:
/// any model with stored `inference_defaults` naming a temperature eats a bare
/// `--presence-penalty` the same way, which is why this warns on every
/// discard rather than only the profile case.
///
/// Never called under `-Q`. `renderer` documents that quiet "suppresses all
/// stderr output … ideal for scripting and piped output", and a warning that
/// broke that contract would be a worse bug than the one it reports.
pub(crate) fn warn_discarded_flags(
    named: &InferenceConfig,
    resolved: &InferenceConfig,
    sources: &gglib_core::domain::FieldSources,
) {
    /// The request occupies rung 0 of the stored ladder — see
    /// `InferenceConfig::resolve_with_profile`.
    const REQUEST_RUNG: usize = 0;

    let discarded = gglib_core::domain::discarded_from_rung(named, resolved, sources, REQUEST_RUNG);
    if discarded.is_empty() {
        return;
    }

    let flags: Vec<String> = discarded
        .iter()
        .map(|field| format!("--{}", field.replace('_', "-")))
        .collect();
    eprintln!(
        "  Warning: {} did not take effect. Sampling penalties travel with \
         whichever layer sets the temperature; pass --temperature to set them together.",
        flags.join(", ")
    );
}
