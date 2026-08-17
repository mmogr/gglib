//! Value parsers for the two reasoning controls, shared by every CLI surface
//! that offers them.
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
