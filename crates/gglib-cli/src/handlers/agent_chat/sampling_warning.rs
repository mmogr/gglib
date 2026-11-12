//! What a session says about the sampling flags its command line named.
//!
//! A chat keeps five of them with its settings (`--temperature`, `--top-p`,
//! `--top-k`, `--max-tokens`, `--repeat-penalty`), which the daemon reads
//! on each of its turns and puts in the request as a client's sampling. The
//! endpoint honours a client's sampling only while "Let clients set
//! sampling" is on, `--max-tokens` always; with it off the session says so,
//! once, before its first turn. Any other sampling flag is kept nowhere, and
//! the session says that it did not take effect.

use gglib_core::Settings;

use crate::shared_args::SamplingArgs;
use crate::target::Target;

/// The lines a session prints on stderr about `sampling`, the flags its
/// command line named, given this machine's `settings`. A session on the
/// paired machine is not told what that machine's endpoint trusts.
pub(crate) fn notices(sampling: &SamplingArgs, target: Target, settings: &Settings) -> Vec<String> {
    let mut lines = Vec::new();
    let trusted = settings.trust_client_sampling.unwrap_or(false);
    let ignored = flags(&kept_but_untrusted(sampling));
    if target == Target::Local && !trusted && !ignored.is_empty() {
        lines.push(format!(
            "  Note: this chat keeps {ignored}, but the endpoint uses its own sampling until \
             \"Let clients set sampling\" is on: gglib config set \
             trust-client-sampling true"
        ));
    }
    let unkept = flags(&not_kept(sampling));
    if !unkept.is_empty() {
        lines.push(format!(
            "  Warning: {unkept} did not take effect: a chat keeps only --temperature, --top-p, \
             --top-k, --max-tokens and --repeat-penalty."
        ));
    }
    lines
}

/// The named flags a chat keeps that the endpoint honours only from a
/// client it trusts: every one it keeps but `--max-tokens`.
fn kept_but_untrusted(s: &SamplingArgs) -> Vec<&'static str> {
    [
        ("--temperature", s.temperature.is_some()),
        ("--top-p", s.top_p.is_some()),
        ("--top-k", s.top_k.is_some()),
        ("--repeat-penalty", s.repeat_penalty.is_some()),
    ]
    .into_iter()
    .filter_map(|(flag, named)| named.then_some(flag))
    .collect()
}

/// The named flags no chat keeps.
fn not_kept(s: &SamplingArgs) -> Vec<&'static str> {
    [
        ("--presence-penalty", s.presence_penalty.is_some()),
        ("--min-p", s.min_p.is_some()),
        ("--frequency-penalty", s.frequency_penalty.is_some()),
        ("--dry-multiplier", s.dry_multiplier.is_some()),
        ("--dry-base", s.dry_base.is_some()),
        ("--dry-allowed-length", s.dry_allowed_length.is_some()),
        ("--dry-penalty-last-n", s.dry_penalty_last_n.is_some()),
        ("--dynatemp-range", s.dynatemp_range.is_some()),
        ("--dynatemp-exponent", s.dynatemp_exponent.is_some()),
        ("--top-n-sigma", s.top_n_sigma.is_some()),
        ("--reasoning-effort", s.reasoning_effort.is_some()),
        (
            "--reasoning-budget-tokens",
            s.reasoning_budget_tokens.is_some(),
        ),
    ]
    .into_iter()
    .filter_map(|(flag, named)| named.then_some(flag))
    .collect()
}

/// `flags` as a person reads a list of them: `--a`, `--a and --b`, or
/// `--a, --b and --c`.
fn flags(flags: &[&str]) -> String {
    match flags {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
#[path = "sampling_warning_tests.rs"]
mod tests;
