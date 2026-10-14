//! Telling the user when a sampling flag did not survive the ladder.
//!
//! Separate from `config` because it is presentation: `config` composes an
//! agent session, and a module that both wires ports and formats stderr is two
//! things.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gglib_core::domain::{InferenceConfig, ParamSource};
use gglib_core::request_pipeline::SamplingDecision;
use gglib_runtime::SamplingObserver;

/// Where a session's flags sit in the ladder the request pipeline folds: its
/// `client` rung, beneath the `cli` rung of a server's own flags, which a
/// chat leaves empty.
const FLAGS_RUNG: usize = 1;

/// Say so, once a session, when the ladder passed over a sampling flag the
/// caller passed.
///
/// The session's first request says it, from the decision that request was
/// sampled by: the ladder is folded where a request is shaped and nowhere
/// before it, so that is the first moment anyone can know.
///
/// What it catches: any flag the fold passed over — see
/// [`gglib_core::domain::discarded_from_rung`] — which for a chat's flags is
/// a float JSON cannot carry (`--presence-penalty NaN`), never sent whatever
/// rung wins. A level the model's template does not read is left out: that
/// gate has its own note. Its first case, `--profile chat --presence-penalty
/// 1.2`, no longer discards anything: the coupling rule passes over only
/// rungs beneath the one that claims the temperature, so a flag above it
/// stands (ADR 0022).
///
/// Never composed under `-Q`. `renderer` documents that quiet "suppresses all
/// stderr output … ideal for scripting and piped output", and a warning that
/// broke that contract would be a worse bug than the one it reports.
pub(crate) fn discarded_flags(named: InferenceConfig) -> SamplingObserver {
    telling(named, |warning| eprintln!("{warning}"))
}

/// [`discarded_flags`], with where the warning is said as an argument.
fn telling(named: InferenceConfig, say: impl Fn(&str) + Send + Sync + 'static) -> SamplingObserver {
    let said = AtomicBool::new(false);
    Arc::new(move |decision| {
        if said.swap(true, Ordering::Relaxed) {
            return;
        }
        if let Some(warning) = warning(&named, decision) {
            say(&warning);
        }
    })
}

/// The warning for the flags in `named` that `decision` passed over, or
/// `None` when it passed over none.
fn warning(named: &InferenceConfig, decision: &SamplingDecision) -> Option<String> {
    let flags: Vec<String> = gglib_core::domain::discarded_from_rung(
        named,
        &decision.resolved,
        &decision.sources,
        FLAGS_RUNG,
    )
    .into_iter()
    // A level the model's template does not read was dropped by that gate,
    // which has its own note.
    .filter(|flag| !dropped_by_template(decision, flag))
    .map(|flag| format!("--{}", flag.replace('_', "-")))
    .collect();
    if flags.is_empty() {
        return None;
    }
    Some(format!(
        "  Warning: {} did not take effect.",
        flags.join(", ")
    ))
}

/// Whether `field` was resolved and then deleted because the model's
/// template never reads it (stage 5b of the pipeline).
fn dropped_by_template(decision: &SamplingDecision, field: &str) -> bool {
    decision
        .sources
        .iter()
        .any(|(name, source)| name == field && source == ParamSource::SuppressedByTemplate)
}

#[cfg(test)]
#[path = "sampling_warning_tests.rs"]
mod tests;
