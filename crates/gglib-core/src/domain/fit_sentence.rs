//! A context fit, said in words: the sentence a person reads under a
//! model's name, and the note beside its Context row.
//!
//! Composed once here from [`ContextFit`]'s figures, served on it, and
//! printed by `gglib model explain`; the GUI shows the served words as
//! they are. Nothing here fits anything: the verdict is the launch's own
//! (`fitted` against `serves_at`), and this only spells it.

use super::context_fit::FitInputs;
use super::launch_plan::ContextFit;
use crate::download::format::format_size;
use crate::server_config::ContextSizeSource;

impl ContextFit {
    /// The fit of a context served at `serves_at` (from `source`), sized by
    /// the fit that reached `fitted` from `inputs`, with its words.
    #[must_use]
    pub fn new(
        serves_at: u64,
        source: ContextSizeSource,
        fitted: Option<u64>,
        inputs: &FitInputs,
    ) -> Self {
        let mut fit = Self {
            serves_at,
            source,
            fitted,
            budget_bytes: inputs.budget_bytes,
            weights_bytes: inputs.weights_bytes,
            kv_bytes_per_token: inputs.kv_bytes_per_token,
            trained_ctx: inputs.trained_ctx,
            unsnapped: inputs.unsnapped,
            sentence: String::new(),
            note: None,
        };
        fit.sentence = fit_sentence(&fit);
        fit.note = fit_note(&fit);
        fit
    }
}

/// The fit sentence: the context the model would be served at, of the
/// window it was trained for, and whether it fits this machine.
///
/// - `Fits this machine with 32K context, of 128K trained.`
/// - `Serves at 64K context, of 128K trained; 32K fits this machine.`
/// - `Serves at 4K context; gglib could not fit one to this machine: …`,
///   naming each figure the fit was missing.
#[must_use]
pub(crate) fn fit_sentence(fit: &ContextFit) -> String {
    let at = tokens(fit.serves_at);
    let trained = fit
        .trained_ctx
        .map_or_else(String::new, |t| format!(", of {} trained", tokens(t)));
    match fit.fitted {
        None => format!(
            "Serves at {at} context{trained}; gglib could not fit one to this machine{}.",
            missing(fit)
        ),
        Some(fitted) if fit.serves_at <= fitted => {
            format!("Fits this machine with {at} context{trained}.")
        }
        Some(fitted) => format!(
            "Serves at {at} context{trained}; {} fits this machine.",
            tokens(fitted)
        ),
    }
}

/// What the served context needs (the weights and a KV cache that size)
/// against the budget, with the verdict: `fits (35.81 GiB of 48.00 GiB)`.
///
/// `None` unless the fit knew every figure it works from: without the
/// trained window the fit refuses whatever the memory, so a verdict beside
/// a sum would say nothing true.
#[must_use]
pub(crate) fn fit_note(fit: &ContextFit) -> Option<String> {
    let budget = fit.budget_bytes?;
    let weights = fit.weights_bytes?;
    let per_token = fit.kv_bytes_per_token?;
    fit.trained_ctx?;
    let need = weights.saturating_add(per_token.saturating_mul(fit.serves_at));
    let word = match fit.fitted {
        Some(fitted) if fit.serves_at <= fitted => "fits",
        _ => "does not fit",
    };
    Some(format!(
        "{word} ({} of {})",
        format_size(need),
        format_size(budget)
    ))
}

/// What a refused fit was missing, from the figures that are `None`:
/// `": a and b"`, or nothing when it knew them all (the smallest rung did
/// not fit). What the model's header leaves out is one clause, so a model
/// that says neither reads "the model does not say what its context costs
/// or what it was trained for".
fn missing(fit: &ContextFit) -> String {
    let unsaid: Vec<&str> = [
        (fit.kv_bytes_per_token.is_none(), "what its context costs"),
        (fit.trained_ctx.is_none(), "what it was trained for"),
    ]
    .into_iter()
    .filter_map(|(gap, words)| gap.then_some(words))
    .collect();
    let model =
        (!unsaid.is_empty()).then(|| format!("the model does not say {}", unsaid.join(" or ")));
    let gaps: Vec<String> = [
        fit.budget_bytes
            .is_none()
            .then(|| "this machine's memory could not be read".to_owned()),
        fit.weights_bytes
            .is_none()
            .then(|| "the weights' size is not known".to_owned()),
        model,
    ]
    .into_iter()
    .flatten()
    .collect();
    match gaps.as_slice() {
        [] => String::new(),
        [one] => format!(": {one}"),
        [first, second] => format!(": {first} and {second}"),
        [init @ .., last] => format!(": {}, and {last}", init.join(", ")),
    }
}

/// A context size: `32K` when it is whole thousands of 1,024, else the
/// number grouped (`30,000`).
fn tokens(n: u64) -> String {
    if n > 0 && n.is_multiple_of(1024) {
        format!("{}K", n / 1024)
    } else {
        grouped(n)
    }
}

/// A whole number with a comma between each group of three digits.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
#[path = "fit_sentence_tests.rs"]
mod tests;
