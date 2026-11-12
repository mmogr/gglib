//! Renders a resolved sampling config alongside the layer that supplied each
//! parameter, for `gglib model explain`.
//!
//! Format-only, like the rest of [`crate::presentation`]: it draws the
//! [`SamplingExplanationDto`] inside what `ModelOps::explain` answers with,
//! the one `GET /api/models/{id}/explain` sends, and re-derives no value,
//! source or published comparison. Plain text with no colour, matching
//! [`super::inspect_display`] — a fact about a model is not a state, so it
//! borrows no state colour.

use std::fmt::Write as _;

use gglib_app_services::{
    ProvenanceKindDto, PublishedDefaultDto, PublishedStateDto, SamplingExplanationDto,
    SamplingLayerDto,
};
use gglib_core::domain::{DefaultsOrigin, ReasoningEffort};
use gglib_core::request_pipeline::CLIENT_AUTHORITATIVE_KEYS;

/// Width of the parameter-name column.
///
/// The longest name is `reasoning_budget_tokens` at 23 characters, so the
/// column is 24 to keep one space before the value. `{:<N}` does not
/// truncate, it just stops padding, so an over-long name would render its
/// value hard against it (`dry_penalty_last_n—`) rather than misaligning
/// visibly enough to be noticed. `every_name_fits_its_column` fails if a
/// longer name is added.
const NAME_WIDTH: usize = 24;

/// Width of the value column, wide enough for `-0.0000` style floats without
/// pushing the source column into a second screen.
///
/// Not widened for `reasoning_budget_tokens`, which is an `i32` and could in
/// principle print eleven characters. Real budgets are `-1` or four to five
/// digits; sizing the column for `i32::MIN` would push every row of the table
/// right to accommodate a number nobody sets. `{:<7}` stops padding rather than
/// truncating, so an absurd value stays readable — it just shifts its own
/// arrow.
const VALUE_WIDTH: usize = 7;

/// Wide enough to span the longest row: the two-space indent, both columns,
/// and the longest source label (`per-model defaults (auto-detected: reasoning
/// tag)`) — 2 + 24 + 7 + 3 + 48 = 84.
///
/// Derived from [`NAME_WIDTH`], and widened with it every time: a name column
/// that grows pushes every row right, and a separator shorter than its own
/// table looks like the table overflowed rather than like the rule was too
/// short. `notes_fit_within_the_table_width` asserts both directions.
const SEP_WIDTH: usize = 85;

/// The arrow separating a value from its provenance.
const ARROW: &str = "\u{2190}";

/// Shown for a parameter that resolved to no value at all.
const ABSENT: &str = "\u{2014}";

/// Indent for a note hanging under its parameter row.
///
/// Not aligned to the value column, which was the first choice and does not
/// fit: `general.sampling.penalty_repeat` is 31 characters, and a note naming
/// it plus both numbers overruns [`SEP_WIDTH`] from that far in. Indenting past
/// the name column is enough to read as a sub-item of the row above.
const NOTE_INDENT: usize = 6;

/// Marks a note that reports gglib displacing the model author's own value.
const MARK_OVERRIDE: char = '!';

/// Marks a note that reports agreement or deferral — present so the reader can
/// see the model published *something*, without it reading as a fault.
const MARK_INFO: char = '\u{b7}';

/// Marks a note gglib cannot draw a conclusion from.
const MARK_UNKNOWN: char = '?';

/// Print the resolved parameters and their provenance.
pub(crate) fn print_explanation(name: &str, id: i64, explanation: &SamplingExplanationDto) {
    print!("{}", explanation_text(name, id, explanation));
}

/// The table as text: its heading, a row per parameter between two rules,
/// and the caveats.
#[must_use]
pub(crate) fn explanation_text(
    name: &str,
    id: i64,
    explanation: &SamplingExplanationDto,
) -> String {
    let rule = "-".repeat(SEP_WIDTH);
    let profile = explanation.profile.as_ref();
    let profile = profile.map_or_else(String::new, |profile| format!(", profile '{profile}'"));

    let mut out = format!("\n  Sampling for {name} (id {id}){profile}\n{rule}\n");
    for line in explanation_lines(explanation) {
        let _ = writeln!(out, "  {line}");
    }
    let _ = writeln!(out, "{rule}");
    for note in caveats(explanation) {
        let _ = writeln!(out, "  {note}");
    }
    out.push('\n');
    out
}

/// The body of the table, one string per parameter.
///
/// Split from the printing so it can be asserted on directly — this is the
/// part with the logic in it.
///
/// # Every provenance row is rendered, and that is checked
///
/// [`SamplingExplanationDto::sources`] arrives in the single display order
/// every provenance surface reads, and this function pairs it with a value
/// column using `zip`, which **truncates**. A field that gains provenance and
/// no value row would disappear from `gglib model explain` with no compile
/// error, no failing count, and no visible symptom — the same shape as a
/// [`NAME_WIDTH`] too narrow for its longest name.
/// `every_provenance_row_is_rendered` and the assertion below say that a row
/// without a value column is a fault, with no register of approved omissions.
#[must_use]
pub(crate) fn explanation_lines(explanation: &SamplingExplanationDto) -> Vec<String> {
    let resolved = &explanation.resolved;
    let values = [
        ("temperature", fmt_f32(resolved.temperature)),
        ("top_p", fmt_f32(resolved.top_p)),
        ("top_k", fmt_i32(resolved.top_k)),
        ("presence_penalty", fmt_f32(resolved.presence_penalty)),
        ("repeat_penalty", fmt_f32(resolved.repeat_penalty)),
        ("min_p", fmt_f32(resolved.min_p)),
        ("frequency_penalty", fmt_f32(resolved.frequency_penalty)),
        ("dynatemp_range", fmt_f32(resolved.dynatemp_range)),
        ("dynatemp_exponent", fmt_f32(resolved.dynatemp_exponent)),
        ("top_n_sigma", fmt_f32(resolved.top_n_sigma)),
        ("dry_multiplier", fmt_f32(resolved.dry_multiplier)),
        ("dry_base", fmt_f32(resolved.dry_base)),
        ("dry_allowed_length", fmt_i32(resolved.dry_allowed_length)),
        ("dry_penalty_last_n", fmt_i32(resolved.dry_penalty_last_n)),
        ("max_tokens", fmt_u32(resolved.max_tokens)),
        ("reasoning_effort", fmt_effort(resolved.reasoning_effort)),
        (
            "reasoning_budget_tokens",
            fmt_i32(resolved.reasoning_budget_tokens),
        ),
    ];

    // The `zip` below truncates, so this is the only thing standing between an
    // unrendered row and silence. See this function's own docs.
    debug_assert_eq!(
        explanation.sources.len(),
        values.len(),
        "a provenance row has no value column and would vanish from the table"
    );

    let rows = explanation.sources.iter().zip(values);
    rows.flat_map(|(source, (field, value))| {
        // The row is named as the CLI's flags are, and its source by the
        // wire's camelCase key: one name, spelled two ways.
        debug_assert_eq!(
            source.param.to_lowercase(),
            field.replace('_', ""),
            "provenance and value rows must stay aligned"
        );
        let row = format!(
            "{field:<NAME_WIDTH$}{value:<VALUE_WIDTH$} {ARROW} {}",
            describe(source.kind, source.layer, explanation)
        );
        let mut published = explanation.published.iter();
        let published = published.find(|entry| entry.param == source.param);
        let note = published
            .map(published_note)
            .map(|n| format!("{:NOTE_INDENT$}{n}", ""));
        std::iter::once(row).chain(note)
    })
    .collect()
}

/// Describe what the model published for one field.
///
/// A field no model can reach (`presence_penalty`, `dry_multiplier`) and one
/// this model left alone have no entry to describe — in both cases there is
/// no author recommendation, so there is nothing to say and a note would be
/// noise on every ordinary model.
fn published_note(entry: &PublishedDefaultDto) -> String {
    let key = &entry.key;
    match entry.state {
        PublishedStateDto::Overridden { published, sending } => format!(
            "{MARK_OVERRIDE} {key} = {}; gglib is sending {}",
            fmt_published(published),
            fmt_published(sending)
        ),
        // Named separately from `Restated` because the row above reads `—`, and
        // a dash with no note is indistinguishable from a gap. This is ADR
        // 0004's follow-up: the missing number is the model's, not nobody's.
        PublishedStateDto::Deferred { published } => format!(
            "{MARK_INFO} {key} = {}; gglib defers to it",
            fmt_published(published)
        ),
        PublishedStateDto::Restated { published } => format!(
            "{MARK_INFO} {key} = {}; gglib sends the same value",
            fmt_published(published)
        ),
        PublishedStateDto::Unreadable => {
            format!("{MARK_UNKNOWN} {key} is set to a value gglib cannot read")
        }
    }
}

/// Render a value that reaches this module as `f64` — either read from the
/// GGUF, or widened out of the request patch.
///
/// Deliberately *not* [`fmt_f32`]'s one-decimal rule. That rule exists so a
/// resolved `1` does not read as a count, but these notes mix `top_k`'s genuine
/// count with the sampling floats, and `general.sampling.top_k = 17.0` would
/// read as an error in the file.
///
/// The precision trim matters more than it looks. gglib's own values are `f32`
/// and reach here through JSON as `f64`, so a resolved temperature of `0.7`
/// arrives as `0.699999988079071` — which would render an ordinary override as
/// though gglib were sending some bizarre high-precision number, and overrun
/// the table besides. `ProxySamplingPanel.formatValue` does the same thing to
/// the same values for the same reason.
fn fmt_published(value: f64) -> String {
    format!("{}", trim_f32_artifact(value))
}

/// Round to six significant digits, the precision an `f32` actually carries.
///
/// The Rust half of `Number(value.toPrecision(6))`.
fn trim_f32_artifact(value: f64) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    let magnitude = value.abs().log10().floor();
    let factor = 10f64.powf(5.0 - magnitude);
    (value * factor).round() / factor
}

/// Name the rung a parameter resolved from, in the user's terms.
///
/// The wording for the model's own defaults matches `inspect_display`'s, so
/// the two commands describe the same stored fact the same way.
fn describe(
    kind: ProvenanceKindDto,
    layer: Option<SamplingLayerDto>,
    explanation: &SamplingExplanationDto,
) -> String {
    match kind {
        ProvenanceKindDto::Layer => match layer {
            // The request rung is always empty for this command — nothing has
            // been asked yet — so reaching it would be a wiring bug.
            Some(SamplingLayerDto::Request) => "request parameters".to_owned(),
            Some(SamplingLayerDto::Profile) => explanation
                .profile
                .as_ref()
                .map_or_else(|| "profile".to_owned(), |name| format!("profile '{name}'")),
            Some(SamplingLayerDto::ModelUserSet) => "per-model defaults (user-set)".to_owned(),
            Some(SamplingLayerDto::Global) => "global settings".to_owned(),
            // A published, a measured and the `reasoning` recipe share this
            // rung — none was reviewed here, so all rank below global
            // settings — and the origin is what names whose numbers they are.
            Some(SamplingLayerDto::ModelAutoDetected) => match explanation.defaults_origin {
                Some(DefaultsOrigin::Published) => {
                    "per-model defaults (published by the model author)".to_owned()
                }
                Some(DefaultsOrigin::Measured) => {
                    "per-model defaults (measured by a tune sweep)".to_owned()
                }
                _ => "per-model defaults (auto-detected: reasoning tag)".to_owned(),
            },
            // A rung the explanation could not name, which none of the five it
            // resolves is.
            None => "an unnamed layer".to_owned(),
        },
        ProvenanceKindDto::Coupled => "unset (coupled to temperature layer)".to_owned(),
        ProvenanceKindDto::Unset => "unset by design".to_owned(),
    }
}

/// Footnotes for the rungs this command cannot see.
///
/// `explain` reports stored configuration. The operator's own `gglib proxy`
/// flags and the client's request parameters are real rungs that outrank
/// everything above, but neither is stored anywhere to read — saying so is
/// cheaper than letting someone conclude the table is the whole story.
///
/// The untrusted note names [`CLIENT_AUTHORITATIVE_KEYS`] rather than
/// spelling the carve-out out, because the two drifted the moment the list
/// stopped being one key long: the sentence said "except `max_tokens`" for a
/// gate that had also gone client-authoritative on `reasoning_budget_tokens`,
/// and nothing failed. Deriving it means the description of the trust
/// boundary cannot be wrong about the boundary again.
///
/// That cost the phrase its "-supplied": both keys spelled out run the line
/// past [`SEP_WIDTH`], and a caveat wider than the rule it sits under reads
/// like the table overflowed. `caveats_fit_within_the_table_width` now holds
/// that, so a third key has to be fitted rather than silently overrun.
fn caveats(explanation: &SamplingExplanationDto) -> Vec<String> {
    let mut notes = vec![
        "Operator flags (gglib proxy --temperature, ...) outrank every layer above.".to_owned(),
    ];
    notes.push(if explanation.trust_client_sampling {
        "Client sampling is trusted and outranks all but those flags.".to_owned()
    } else {
        format!(
            "Client sampling is ignored, except {}.",
            CLIENT_AUTHORITATIVE_KEYS.join(", ")
        )
    });
    notes
}

/// Render a sampling float, keeping one decimal on whole numbers.
///
/// `1` and `0` read as integers and invite the reader to wonder whether the
/// parameter is a count; `1.0` and `0.0` read as the sampling values they are,
/// and match how every other surface prints them.
fn fmt_f32(value: Option<f32>) -> String {
    value.map_or_else(
        || ABSENT.to_owned(),
        |v| {
            if v.fract().abs() < f32::EPSILON {
                format!("{v:.1}")
            } else {
                format!("{v}")
            }
        },
    )
}

fn fmt_i32(value: Option<i32>) -> String {
    value.map_or_else(|| ABSENT.to_owned(), |v| format!("{v}"))
}

/// Render a reasoning level by its own wire name.
///
/// Deliberately not annotated with the sentinel meanings that
/// `reasoning_budget_tokens` has: an effort level is a word, not a number with
/// magic values, and `-1`-style commentary belongs on the budget row's own
/// note if it ever gains one.
fn fmt_effort(value: Option<ReasoningEffort>) -> String {
    value.map_or_else(|| ABSENT.to_owned(), |v| v.to_string())
}

fn fmt_u32(value: Option<u32>) -> String {
    value.map_or_else(|| ABSENT.to_owned(), |v| format!("{v}"))
}

#[cfg(test)]
#[path = "explain_display_tests.rs"]
mod explain_display_tests;
