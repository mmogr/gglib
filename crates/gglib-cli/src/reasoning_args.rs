//! Value parsers for the two reasoning controls, and the note a level the
//! model's template does not list earns, shared by every CLI surface that
//! offers them.
//!
//! Three surfaces spell the same pair of flags — `SamplingArgs` (per run),
//! `gglib model update` (per model) and `gglib config profile set` (per
//! profile) — and a parser that only one of them used would be a rejection
//! message the other two did not give. So the parsing, and more importantly
//! the *refusals*, live here once.
//!
//! Both refusals are quotations rather than inventions. The effort vocabulary
//! is [`ReasoningEffort`]'s, and the budget range is llama-server's own
//! (`-1..=i32::MAX`, measured in [ADR 0007] finding 7c): a value below `-1`
//! comes back from upstream as a clean HTTP 400 naming that range, so the CLI
//! says the same thing rather than a paraphrase of it.
//!
//! [ADR 0007]: https://github.com/mmogr/gglib/blob/main/docs/adr/0007-ask-the-server-for-template-capabilities.md

use gglib_core::domain::ReasoningEffort;

/// What `--reasoning-budget-tokens` refuses, in upstream's words.
///
/// Kept identical to `read_reasoning_budget_tokens`' `expected` string in
/// `gglib_core::domain::inference`, so a value refused at the flag and the
/// same value refused over HTTP are refused with one sentence.
pub(crate) const BUDGET_RANGE: &str =
    "an integer >= -1 (-1 defers to the launch default, 0 stops thinking)";

/// Parse a `--reasoning-effort` level.
///
/// # Errors
///
/// Returns the accepted vocabulary for anything outside it — and, for
/// `"none"`, points at the flag that actually turns thinking off. `"none"` is
/// the one wrong answer a user is likely to *reason their way to*: upstream
/// accepts it, and it erases the kwarg so the template's own fallback fires
/// (medium, on `gpt-oss`). A bare "not a level" would leave them believing
/// they had switched reasoning off.
pub(crate) fn parse_effort(s: &str) -> Result<ReasoningEffort, String> {
    if let Some(level) = ReasoningEffort::from_wire(s) {
        return Ok(level);
    }
    if s.eq_ignore_ascii_case("none") {
        return Err(format!(
            "'none' is not a level — upstream erases the setting and the template's \
             own default fires instead (medium, on gpt-oss). To stop thinking, pass \
             `--reasoning-budget-tokens 0`. Levels: {}",
            ReasoningEffort::wire_vocabulary()
        ));
    }
    Err(format!(
        "expected one of: {}",
        ReasoningEffort::wire_vocabulary()
    ))
}

/// The note `--reasoning-effort <level>` prints for a model whose template's
/// list is known and does not hold `level`, or `None` when the list holds it
/// or is unknown. `listed` is the model's list
/// (`gglib_core::domain::reasoning_effort_levels`, `None` when unknown), and
/// `done` says what happens to the level anyway: `"is stored"`, `"would be
/// stored"` or `"is sent"`.
///
/// A note and never a refusal: a level the list leaves out can still work
/// (`high` on Qwen3.8), so the list alone cannot say it fails, and a
/// template's capabilities are read, never used to edit a request.
pub(crate) fn unlisted_effort_note(
    level: ReasoningEffort,
    listed: Option<&[ReasoningEffort]>,
    done: &str,
) -> Option<String> {
    let listed = listed?;
    if listed.contains(&level) {
        return None;
    }
    let lists = if listed.is_empty() {
        "lists no reasoning-effort level".to_owned()
    } else {
        let names: Vec<&str> = listed.iter().map(|l| l.as_str()).collect();
        format!("lists the reasoning-effort levels {}", names.join(", "))
    };
    Some(format!(
        "Note: this model's template {lists}, and not {level}. \
         The level {done} all the same: one the list leaves out can still work."
    ))
}

/// Parse a `--reasoning-budget-tokens` value.
///
/// # Errors
///
/// Rejects exactly what upstream rejects — anything below `-1` — and nothing
/// else. `i32::MAX` is upstream's ceiling and clap's `i32` parse enforces it.
pub(crate) fn parse_budget(s: &str) -> Result<i32, String> {
    let n: i32 = s.parse().map_err(|_| format!("expected {BUDGET_RANGE}"))?;
    if n < -1 {
        return Err(format!("expected {BUDGET_RANGE}"));
    }
    Ok(n)
}

#[cfg(test)]
#[path = "reasoning_args_tests.rs"]
mod reasoning_args_tests;
