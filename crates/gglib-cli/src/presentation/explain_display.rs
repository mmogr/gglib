//! Renders a resolved sampling config alongside the layer that supplied each
//! parameter, for `gglib model explain`.
//!
//! Format-only, like the rest of [`crate::presentation`]: the resolution
//! itself happens in `gglib-core`, and nothing here re-derives a value or a
//! source. Plain text with no colour, matching [`super::inspect_display`] —
//! a fact about a model is not a state, so it borrows no state colour.

use gglib_core::domain::{
    DefaultsOrigin, FieldSources, InferenceConfig, ModelSamplingDefaults, ParamSource,
    ReasoningEffort, SamplingLayer, SamplingOverride,
};
use gglib_core::request_pipeline::{CLIENT_AUTHORITATIVE_KEYS, SuppressedEffort};

use super::tables::print_separator;

/// Width of the parameter-name column.
///
/// The longest name is `reasoning_budget_tokens` at 23 characters, so the
/// column is 24 to keep one space before the value.
///
/// It was 17, on a comment claiming `presence_penalty` at 16 was the longest —
/// true when it was written, and false since the four DRY parameters landed;
/// then 19 for those. `{:<19}` does not truncate, it just stops padding, so an
/// over-long name renders its value hard against it
/// (`dry_penalty_last_n—`) rather than misaligning visibly enough to be
/// noticed. `every_name_fits_its_column` fails if a longer name is added.
///
/// This width is the whole of what kept the two reasoning rows out of the
/// table: they were listed as deferred with a note that rendering
/// `reasoning_budget_tokens` needed four more characters here. It needed five,
/// and a wider separator to match, and nothing else.
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

/// How the model's own defaults should be described, once resolved.
///
/// The wording matches `inspect_display`'s, so the two commands describe the
/// same stored fact the same way.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExplainContext<'a> {
    /// The profile that was selected, if any.
    pub profile: Option<&'a str>,
    /// Whether the model carries the `reasoning` tag, which selects the floor.
    pub is_reasoning: bool,
    /// Whether the client's own sampling parameters are honoured at request
    /// time. Shown as a caveat, since this command explains stored
    /// configuration and cannot see a live request.
    pub trust_client_sampling: bool,
    /// What this model's own GGUF publishes, so a row can say whether gglib is
    /// displacing the model author's recommendation.
    ///
    /// Since llama.cpp PR #17120 a `general.sampling.*` key becomes the
    /// server's default for every field gglib does not name, so the provenance
    /// column alone is no longer the whole story: `unset by design` means *the
    /// model's own number applies* on a model that published one, and means
    /// *the build's default applies* on a model that did not. Without this the
    /// two render identically.
    pub model_sampling: ModelSamplingDefaults,
    /// Where the model's stored defaults came from.
    ///
    /// `DefaultsOrigin::Published` and `DefaultsOrigin::AutoDetected` share a
    /// ladder rung — both are unreviewed, so both rank below global settings —
    /// which means the rung alone cannot name its own source. Without this, a
    /// recipe fetched from the model author renders as "auto-detected:
    /// reasoning tag", crediting gglib's guess for somebody else's numbers.
    pub defaults_origin: Option<DefaultsOrigin>,
    /// The `reasoning_effort` this model's template would ignore, when the
    /// stored configuration resolves one it does not read.
    ///
    /// Needed for the same reason the HTTP DTO needs it: by the time a
    /// suppression reaches this module, `resolved.reasoning_effort` is `None`
    /// and the rung in `sources` has been overwritten with the marker, so the
    /// table can say *that* a level was suppressed and neither which one nor
    /// whose. The note hanging under the row is where those two go.
    pub effort_suppressed: Option<SuppressedEffort>,
}

/// Print the resolved parameters and their provenance.
pub(crate) fn print_explanation(
    model_name: &str,
    model_id: i64,
    resolved: &InferenceConfig,
    sources: &FieldSources,
    ctx: ExplainContext<'_>,
) {
    println!();
    match ctx.profile {
        Some(name) => println!("  Sampling for {model_name} (id {model_id}), profile '{name}'"),
        None => println!("  Sampling for {model_name} (id {model_id})"),
    }
    print_separator(SEP_WIDTH);

    for line in explanation_lines(resolved, sources, ctx) {
        println!("  {line}");
    }

    print_separator(SEP_WIDTH);
    for note in caveats(ctx) {
        println!("  {note}");
    }
    println!();
}

/// The body of the table, one string per parameter.
///
/// Split from the printing so it can be asserted on directly — this is the
/// part with the logic in it.
///
/// # Every provenance row is rendered, and that is now checked
///
/// [`FieldSources::iter`] is the single display order every provenance surface
/// reads, and this function pairs it with a value column using `zip`, which
/// **truncates**. A field that gains provenance and no value row disappears
/// from `gglib model explain` with no compile error, no failing count, and no
/// visible symptom — the same shape as the [`NAME_WIDTH`] mis-sizing above,
/// which rendered wrongly for months without misaligning enough to be noticed.
///
/// The two reasoning controls used to be a listed exception here, waiting on
/// exactly the column width the constant above now has. The list is gone with
/// them: `every_provenance_row_is_rendered` and the assertion below say that a
/// row without a value column is a fault, with no register of approved
/// omissions to add the next one to.
#[must_use]
pub(crate) fn explanation_lines(
    resolved: &InferenceConfig,
    sources: &FieldSources,
    ctx: ExplainContext<'_>,
) -> Vec<String> {
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
        // The value column shows what would be *sent*, so a suppressed effort
        // reads as a dash here and the note below carries the level. That is
        // `Deferred`'s idiom, not a new one: a dash with an explanation
        // underneath already means "the number that applies is not gglib's".
        ("reasoning_effort", fmt_effort(resolved.reasoning_effort)),
        (
            "reasoning_budget_tokens",
            fmt_i32(resolved.reasoning_budget_tokens),
        ),
    ];

    // What gglib actually puts on the wire, read from the patch the request
    // pipeline merges into the body rather than from the struct fields. A
    // parameter missing from this map is one gglib names nowhere, which is
    // precisely the condition under which the model's own GGUF value survives
    // to the sampler. Deriving it any other way would let this table and the
    // request disagree.
    let patch = resolved.to_openai_json_patch();

    // The `zip` below truncates, so this is the only thing standing between an
    // unrendered row and silence. See this function's own docs.
    debug_assert_eq!(
        sources.iter().count(),
        values.len(),
        "a FieldSources row has no value column and would vanish from the table"
    );

    sources
        .iter()
        .zip(values)
        .flat_map(|((field, source), (value_field, value))| {
            debug_assert_eq!(
                field, value_field,
                "provenance and value rows must stay aligned"
            );
            let row = format!(
                "{field:<NAME_WIDTH$}{value:<VALUE_WIDTH$} {ARROW} {}",
                describe(source, ctx)
            );
            // Two kinds of note can hang under a row and they never collide:
            // the published comparison only ever fires on a field with a GGUF
            // key, and no reasoning control has one.
            let sending = patch.get(field).and_then(serde_json::Value::as_f64);
            let note = suppression_note(field, ctx)
                .or_else(|| published_note(&ctx.model_sampling.compare_field(field, sending)))
                .map(|n| format!("{:NOTE_INDENT$}{n}", ""));
            std::iter::once(row).chain(note)
        })
        .collect()
}

/// Name the level a suppression threw away and the rung that asked for it.
///
/// `None` for every field but `reasoning_effort`, and for that one unless this
/// model's template positively does not read it.
///
/// # Why the level is down here and not in the value column
///
/// The row above reads `reasoning_effort   —   ← suppressed by this model's
/// template`, and a reader could stop there and know the control is inert. What
/// they could not know is **whose setting** is inert — and that is the
/// actionable half. A `:high` profile that does nothing on this model is a fact
/// about the profile, and the person who has to change something is the one who
/// wrote it.
///
/// The level and the rung are both unrecoverable by the time the table is
/// drawn: the gate clears `resolved.reasoning_effort` and overwrites the rung in
/// `sources`. They arrive on [`ExplainContext::effort_suppressed`] or not at
/// all.
///
/// # The tense, and why the note is as terse as it is
///
/// This command explains *stored* configuration and has sent nothing, so
/// nothing here may claim a request happened. The conditional is carried by the
/// row above — "suppressed by this model's template" is a standing property of
/// the model, true of every request and of none in particular — which lets this
/// note be a label rather than a sentence.
///
/// That terseness is also forced. The longest rung label is 48 characters
/// (`per-model defaults (auto-detected: reasoning tag)`), and after the
/// [`NOTE_INDENT`] and the print indent there are 77 left of [`SEP_WIDTH`] —
/// so a fuller phrasing overruns the rule it sits under on exactly the model
/// that needs the note most. `the_reasoning_rows_and_their_note_fit_the_table_width`
/// holds the corner.
fn suppression_note(field: &str, ctx: ExplainContext<'_>) -> Option<String> {
    if field != "reasoning_effort" {
        return None;
    }
    let suppressed = ctx.effort_suppressed?;
    Some(format!(
        "{MARK_OVERRIDE} '{}' from {}; not sent",
        suppressed.level,
        describe(suppressed.source, ctx)
    ))
}

/// Describe what the model published for one field, if it published anything.
///
/// `None` for a field no model can reach (`presence_penalty`,
/// `dry_multiplier`) and for one this model left alone — in both cases there is
/// no author recommendation, so there is nothing to say and a note would be
/// noise on every ordinary model.
fn published_note(verdict: &SamplingOverride) -> Option<String> {
    match verdict {
        SamplingOverride::NotPublished => None,
        SamplingOverride::Overridden {
            key,
            published,
            sending,
        } => Some(format!(
            "{MARK_OVERRIDE} {key} = {}; gglib is sending {}",
            fmt_published(*published),
            fmt_published(*sending)
        )),
        // Named separately from `Restated` because the row above reads `—`, and
        // a dash with no note is indistinguishable from a gap. This is ADR
        // 0004's follow-up: the missing number is the model's, not nobody's.
        SamplingOverride::Deferred { key, published } => Some(format!(
            "{MARK_INFO} {key} = {}; gglib defers to it",
            fmt_published(*published)
        )),
        SamplingOverride::Restated { key, published } => Some(format!(
            "{MARK_INFO} {key} = {}; gglib sends the same value",
            fmt_published(*published)
        )),
        SamplingOverride::Unreadable { key, .. } => Some(format!(
            "{MARK_UNKNOWN} {key} is set to a value gglib cannot read"
        )),
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
fn describe(source: ParamSource, ctx: ExplainContext<'_>) -> String {
    match source {
        ParamSource::Layer(index) => match SamplingLayer::from_index(index) {
            // The request rung is always empty for this command — nothing has
            // been asked yet — so reaching it would be a wiring bug.
            Some(SamplingLayer::Request) => "request parameters".to_owned(),
            Some(SamplingLayer::Profile) => ctx
                .profile
                .map_or_else(|| "profile".to_owned(), |name| format!("profile '{name}'")),
            Some(SamplingLayer::ModelUserSet) => "per-model defaults (user-set)".to_owned(),
            Some(SamplingLayer::Global) => "global settings".to_owned(),
            Some(SamplingLayer::ModelAutoDetected) => match ctx.defaults_origin {
                Some(DefaultsOrigin::Published) => {
                    "per-model defaults (published by the model author)".to_owned()
                }
                Some(DefaultsOrigin::Measured) => {
                    "per-model defaults (measured by a tune sweep)".to_owned()
                }
                _ => "per-model defaults (auto-detected: reasoning tag)".to_owned(),
            },
            None => format!("layer {index}"),
        },
        ParamSource::Floor => format!("{} floor", floor_name(ctx)),
        ParamSource::FloorCoupled => {
            format!("{} floor (coupled to temperature layer)", floor_name(ctx))
        }
        ParamSource::Unset => "unset by design".to_owned(),
        // Reachable, and the reason this table exists for the reasoning
        // controls at all. `explain` resolves stored configuration with no
        // request in hand — but the *template* is a property of the model, not
        // of the request, so the question "would this level survive?" can be
        // answered from the catalog row alone. `explain` applies the same
        // predicate the pipeline's stage 5b applies, via the same function.
        //
        // Never folded into "unset by design": the two differ in whether a rung
        // named something, which is the whole content of the suppression, and
        // "unset" would report that nobody configured a control somebody
        // configured.
        ParamSource::SuppressedByTemplate => "suppressed by this model's template".to_owned(),
    }
}

/// Which of the two class floors applies, named so the difference is visible.
const fn floor_name(ctx: ExplainContext<'_>) -> &'static str {
    if ctx.is_reasoning {
        "reasoning"
    } else {
        "default"
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
/// stopped being one key long: the sentence said "except max_tokens" for a
/// gate that had also gone client-authoritative on `reasoning_budget_tokens`,
/// and nothing failed. Deriving it means the description of the trust
/// boundary cannot be wrong about the boundary again.
///
/// That cost the phrase its "-supplied": both keys spelled out run the line
/// past [`SEP_WIDTH`], and a caveat wider than the rule it sits under reads
/// like the table overflowed. `caveats_fit_within_the_table_width` now holds
/// that, so a third key has to be fitted rather than silently overrun.
fn caveats(ctx: ExplainContext<'_>) -> Vec<String> {
    let mut notes = vec![
        "Operator flags (gglib proxy --temperature, ...) outrank every layer above.".to_owned(),
    ];
    notes.push(if ctx.trust_client_sampling {
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
