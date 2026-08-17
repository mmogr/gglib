//! Renders a resolved sampling config alongside the layer that supplied each
//! parameter, for `gglib model explain`.
//!
//! Format-only, like the rest of [`crate::presentation`]: the resolution
//! itself happens in `gglib-core`, and nothing here re-derives a value or a
//! source. Plain text with no colour, matching [`super::inspect_display`] —
//! a fact about a model is not a state, so it borrows no state colour.

use gglib_core::domain::{
    DefaultsOrigin, FieldSources, InferenceConfig, ModelSamplingDefaults, ParamSource,
    SamplingLayer, SamplingOverride,
};
use gglib_core::request_pipeline::CLIENT_AUTHORITATIVE_KEYS;

use super::tables::print_separator;

/// Width of the parameter-name column.
///
/// The longest names are `dry_allowed_length` and `dry_penalty_last_n` at 18
/// characters, so the column is 19 to keep one space before the value.
///
/// It was 17, on a comment claiming `presence_penalty` at 16 was the longest —
/// true when it was written, and false since the four DRY parameters landed.
/// `{:<17}` does not truncate, it just stops padding, so both DRY rows rendered
/// their value hard against the name (`dry_penalty_last_n—`) rather than
/// misaligning visibly enough to be noticed. `every_name_fits_its_column`
/// now fails if a longer name is added.
const NAME_WIDTH: usize = 19;

/// Width of the value column, wide enough for `-0.0000` style floats without
/// pushing the source column into a second screen.
const VALUE_WIDTH: usize = 7;

/// Wide enough to span the longest row: the two-space indent, both columns,
/// and the longest source label (`per-model defaults (auto-detected: reasoning
/// tag)`) — 2 + 19 + 7 + 3 + 48 = 79.
///
/// Widened from 78 with [`NAME_WIDTH`], which it is derived from: a name column
/// that grows pushes every row right, and a separator shorter than its own
/// table looks like the table overflowed rather than like the rule was too
/// short. `notes_fit_within_the_table_width` asserts both directions.
const SEP_WIDTH: usize = 80;

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

/// Provenance rows this table knowingly does not render.
///
/// [`FieldSources::iter`] is the single display order every provenance surface
/// reads, and [`explanation_lines`] pairs it with a value column using `zip`,
/// which **truncates**. So a field that gains provenance and no value row
/// disappears from `gglib model explain` with no compile error, no failing
/// count, and no visible symptom — the same shape as the [`NAME_WIDTH`]
/// mis-sizing above, which also rendered wrongly for months without
/// misaligning enough to be noticed.
///
/// The two reasoning controls sit here deliberately. They joined the sampling
/// ladder ahead of any surface that can set them, and rendering
/// `reasoning_budget_tokens` needs a name column four characters wider than
/// this table has — a layout change that belongs with the flags, not with the
/// ladder. Listing them makes the gap a to-do a reader can see;
/// `every_provenance_row_is_rendered_or_explicitly_deferred` fails if a third
/// field goes quiet, and fails again when these two gain their rows and this
/// list is not emptied.
const DEFERRED_ROWS: [&str; 2] = ["reasoning_effort", "reasoning_budget_tokens"];

/// The body of the table, one string per parameter.
///
/// Split from the printing so it can be asserted on directly — this is the
/// part with the logic in it.
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
    ];

    // What gglib actually puts on the wire, read from the patch the request
    // pipeline merges into the body rather than from the struct fields. A
    // parameter missing from this map is one gglib names nowhere, which is
    // precisely the condition under which the model's own GGUF value survives
    // to the sampler. Deriving it any other way would let this table and the
    // request disagree.
    let patch = resolved.to_openai_json_patch();

    // The `zip` below truncates, so this is the only thing standing between an
    // unrendered row and silence. See `DEFERRED_ROWS`.
    debug_assert_eq!(
        sources.iter().count(),
        values.len() + DEFERRED_ROWS.len(),
        "a FieldSources row has no value column and is not in DEFERRED_ROWS"
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
            let sending = patch.get(field).and_then(serde_json::Value::as_f64);
            let note = published_note(&ctx.model_sampling.compare_field(field, sending))
                .map(|n| format!("{:NOTE_INDENT$}{n}", ""));
            std::iter::once(row).chain(note)
        })
        .collect()
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

fn fmt_u32(value: Option<u32>) -> String {
    value.map_or_else(|| ABSENT.to_owned(), |v| format!("{v}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ExplainContext<'static> {
        ExplainContext {
            profile: None,
            is_reasoning: false,
            trust_client_sampling: false,
            model_sampling: ModelSamplingDefaults::default(),
            defaults_origin: None,
        }
    }

    /// A context for a model that published `general.sampling.*` keys.
    fn ctx_publishing(pairs: &[(&str, &str)]) -> ExplainContext<'static> {
        let metadata: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        ExplainContext {
            model_sampling: ModelSamplingDefaults::from_metadata(&metadata),
            ..ctx()
        }
    }

    /// The line reporting what the model published for `field`, if any.
    fn note_for(lines: &[String], field: &str) -> Option<String> {
        let row = lines.iter().position(|l| l.starts_with(field))?;
        lines
            .get(row + 1)
            .filter(|l| l.starts_with(' '))
            .map(|l| l.trim().to_owned())
    }

    /// A model whose auto-detected recipe claims the temperature, with global
    /// settings winning the one parameter it left alone — the shape this
    /// command exists to make legible.
    fn auto_detected_sources() -> FieldSources {
        FieldSources {
            temperature: ParamSource::Layer(4),
            top_p: ParamSource::Layer(4),
            top_k: ParamSource::Layer(3),
            presence_penalty: ParamSource::Layer(4),
            repeat_penalty: ParamSource::FloorCoupled,
            min_p: ParamSource::FloorCoupled,
            dry_multiplier: ParamSource::FloorCoupled,
            dynatemp_range: ParamSource::Unset,
            dynatemp_exponent: ParamSource::Unset,
            top_n_sigma: ParamSource::Unset,
            dry_base: ParamSource::Unset,
            dry_allowed_length: ParamSource::Unset,
            dry_penalty_last_n: ParamSource::Unset,
            frequency_penalty: ParamSource::Unset,
            max_tokens: ParamSource::Unset,
            reasoning_effort: ParamSource::Unset,
            reasoning_budget_tokens: ParamSource::Unset,
        }
    }

    /// The gap in [`DEFERRED_ROWS`] is a listed one, not a leak.
    ///
    /// `explanation_lines` pairs provenance with values by `zip`, which
    /// truncates silently — a field added to `FieldSources` and not to the
    /// value column vanishes from this table with no compile error and no
    /// failing count. This asserts that every row `FieldSources` carries is
    /// either rendered or explicitly deferred, so the *next* omission fails
    /// here instead of shipping.
    #[test]
    fn every_provenance_row_is_rendered_or_explicitly_deferred() {
        let lines = explanation_lines(
            &InferenceConfig::with_hardcoded_defaults(),
            &auto_detected_sources(),
            ctx(),
        );

        for (field, _) in auto_detected_sources().iter() {
            let rendered = lines.iter().any(|line| line.starts_with(field));
            let deferred = DEFERRED_ROWS.contains(&field);
            assert!(
                rendered != deferred,
                "{field}: rendered={rendered} deferred={deferred} — a field must be \
                 one or the other, never both and never neither"
            );
        }
    }

    #[test]
    fn every_parameter_gets_exactly_one_line() {
        let lines = explanation_lines(
            &InferenceConfig::with_hardcoded_defaults(),
            &auto_detected_sources(),
            ctx(),
        );
        assert_eq!(lines.len(), 15, "{lines:#?}");
        for field in [
            "temperature",
            "top_p",
            "top_k",
            "presence_penalty",
            "repeat_penalty",
            "min_p",
            "dry_multiplier",
            "dry_base",
            "dry_allowed_length",
            "dry_penalty_last_n",
            "max_tokens",
        ] {
            assert_eq!(
                lines.iter().filter(|l| l.starts_with(field)).count(),
                1,
                "{field} should appear once in {lines:#?}"
            );
        }
    }

    /// The two ranks of per-model defaults must read differently — telling
    /// them apart is the whole point of the #685 distinction.
    #[test]
    fn user_set_and_auto_detected_defaults_are_worded_differently() {
        let user = describe(ParamSource::Layer(2), ctx());
        let auto = describe(ParamSource::Layer(4), ctx());

        assert!(user.contains("user-set"), "{user}");
        assert!(auto.contains("auto-detected"), "{auto}");
        assert_ne!(user, auto);
    }

    /// A floor reached because the coupling rule suppressed the layers below
    /// must not look like one nobody ever set a value for.
    #[test]
    fn a_coupled_floor_says_why_it_is_the_floor() {
        let plain = describe(ParamSource::Floor, ctx());
        let coupled = describe(ParamSource::FloorCoupled, ctx());

        assert!(!plain.contains("coupled"), "{plain}");
        assert!(
            coupled.contains("coupled to temperature layer"),
            "{coupled}"
        );
    }

    /// A reasoning model sits on a different floor, and the value alone does
    /// not say so.
    #[test]
    fn the_floor_is_named_so_the_two_are_distinguishable() {
        let reasoning = ExplainContext {
            is_reasoning: true,
            ..ctx()
        };
        assert!(describe(ParamSource::Floor, reasoning).contains("reasoning floor"));
        assert!(describe(ParamSource::Floor, ctx()).contains("default floor"));
    }

    /// `max_tokens` has no floor value on purpose; it must read as a decision
    /// rather than as a missing number.
    #[test]
    fn an_absent_max_tokens_reads_as_deliberate() {
        let resolved = InferenceConfig::with_hardcoded_defaults();
        assert_eq!(resolved.max_tokens, None, "guards the premise");

        let lines = explanation_lines(&resolved, &auto_detected_sources(), ctx());
        let line = lines
            .iter()
            .find(|l| l.starts_with("max_tokens"))
            .expect("max_tokens is rendered");

        assert!(line.contains(ABSENT), "{line}");
        assert!(line.contains("unset by design"), "{line}");
    }

    /// The profile rung is named after the profile the user actually asked
    /// for, not the generic word.
    #[test]
    fn the_profile_rung_carries_the_selected_name() {
        let with_profile = ExplainContext {
            profile: Some("coding"),
            ..ctx()
        };
        assert_eq!(
            describe(ParamSource::Layer(1), with_profile),
            "profile 'coding'"
        );
    }

    /// The value column shows the resolved number, not the source's.
    #[test]
    fn values_come_from_the_resolved_config() {
        let resolved = InferenceConfig {
            temperature: Some(0.2),
            top_k: Some(20),
            ..InferenceConfig::with_hardcoded_defaults()
        };
        let lines = explanation_lines(&resolved, &auto_detected_sources(), ctx());

        assert!(lines[0].contains("0.2"), "{}", lines[0]);
        assert!(lines[2].contains("20"), "{}", lines[2]);
    }

    /// Sampling floats keep a decimal so they do not read as counts, while
    /// genuinely integral parameters stay integral.
    #[test]
    fn whole_sampling_floats_keep_one_decimal() {
        assert_eq!(fmt_f32(Some(1.0)), "1.0");
        assert_eq!(fmt_f32(Some(0.0)), "0.0");
        assert_eq!(fmt_f32(Some(0.95)), "0.95");
        assert_eq!(fmt_f32(Some(1.5)), "1.5");
        assert_eq!(fmt_f32(None), ABSENT);

        // top_k and max_tokens are counts and must not gain a decimal.
        assert_eq!(fmt_i32(Some(20)), "20");
        assert_eq!(fmt_u32(Some(8192)), "8192");
    }

    // =========================================================================
    // What the model published
    // =========================================================================

    /// The headline case. A `reasoning` model's auto-detected recipe names
    /// `temperature: 1.0`; the model's own GGUF asks for `0.33`. gglib wins on
    /// the wire, and the table has to say so rather than showing `1.0` beside a
    /// provenance label that never mentions the model author.
    #[test]
    fn an_overridden_published_value_names_both_numbers() {
        let resolved = InferenceConfig {
            temperature: Some(1.0),
            ..InferenceConfig::with_hardcoded_defaults()
        };
        let lines = explanation_lines(
            &resolved,
            &auto_detected_sources(),
            ctx_publishing(&[("general.sampling.temp", "0.33")]),
        );

        let note = note_for(&lines, "temperature").expect("temperature carries a note");
        assert!(note.starts_with(MARK_OVERRIDE), "{note}");
        assert!(note.contains("general.sampling.temp = 0.33"), "{note}");
        assert!(note.contains("gglib is sending 1"), "{note}");
    }

    /// **ADR 0004's follow-up, and the reason a note is shown at all for a
    /// benign case.** A deferred field renders as `—`, which reads as "nothing
    /// set" — when in fact the model's own number is what the sampler will use.
    #[test]
    fn a_deferred_field_says_the_missing_number_is_the_models() {
        let resolved = InferenceConfig {
            top_k: None,
            ..InferenceConfig::with_hardcoded_defaults()
        };
        let lines = explanation_lines(
            &resolved,
            &FieldSources {
                top_k: ParamSource::Unset,
                ..auto_detected_sources()
            },
            ctx_publishing(&[("general.sampling.top_k", "17")]),
        );

        let row = lines
            .iter()
            .find(|l| l.starts_with("top_k"))
            .expect("top_k is rendered");
        assert!(
            row.contains(ABSENT),
            "the value column is still a dash: {row}"
        );

        let note = note_for(&lines, "top_k").expect("top_k carries a note");
        assert!(note.contains("general.sampling.top_k = 17"), "{note}");
        assert!(note.contains("defers to it"), "{note}");
        assert!(
            !note.starts_with(MARK_OVERRIDE),
            "deferral is not an override: {note}"
        );
    }

    /// An integer key must not grow a decimal — `general.sampling.top_k = 17.0`
    /// reads as a defect in the file.
    #[test]
    fn a_published_integer_keeps_its_integer_form() {
        assert_eq!(fmt_published(17.0), "17");
        assert_eq!(fmt_published(0.33), "0.33");
    }

    /// **The artifact this note would otherwise publish.** gglib's values are
    /// `f32` and arrive here widened, so an ordinary `0.7` reads as
    /// `0.699999988079071` — which looks like a defect and overruns the table.
    #[test]
    fn an_f32_widened_value_renders_as_the_number_it_is() {
        assert_eq!(fmt_published(f64::from(0.7_f32)), "0.7");
        assert_eq!(fmt_published(f64::from(0.05_f32)), "0.05");
        assert_eq!(fmt_published(f64::from(1.07_f32)), "1.07");
        assert_eq!(fmt_published(f64::from(1.5_f32)), "1.5");

        // Small values must keep their significant digits rather than being
        // rounded toward zero — `min_p` is routinely three decimals.
        assert_eq!(fmt_published(f64::from(0.011_f32)), "0.011");

        assert_eq!(fmt_published(0.0), "0");
    }

    /// gglib restating a value it agrees with is not a fault and must not carry
    /// the override marker, but it is still worth seeing.
    #[test]
    fn a_restated_value_is_marked_as_information_not_as_an_override() {
        let resolved = InferenceConfig {
            temperature: Some(0.7),
            ..InferenceConfig::with_hardcoded_defaults()
        };
        let lines = explanation_lines(
            &resolved,
            &auto_detected_sources(),
            ctx_publishing(&[("general.sampling.temp", "0.7")]),
        );

        let note = note_for(&lines, "temperature").expect("temperature carries a note");
        assert!(note.starts_with(MARK_INFO), "{note}");
        assert!(note.contains("the same value"), "{note}");
    }

    /// A value gglib cannot parse must read as unknown, never as an override —
    /// gglib does not know what it displaced.
    #[test]
    fn an_unreadable_published_value_reads_as_unknown() {
        let lines = explanation_lines(
            &InferenceConfig::with_hardcoded_defaults(),
            &auto_detected_sources(),
            ctx_publishing(&[("general.sampling.temp", "warm")]),
        );

        let note = note_for(&lines, "temperature").expect("temperature carries a note");
        assert!(note.starts_with(MARK_UNKNOWN), "{note}");
        assert!(note.contains("cannot read"), "{note}");
    }

    /// The ordinary model — nothing published — must render exactly as before.
    /// A note on every row would train the reader to ignore all of them.
    #[test]
    fn a_model_that_publishes_nothing_gains_no_notes() {
        let lines = explanation_lines(
            &InferenceConfig::with_hardcoded_defaults(),
            &auto_detected_sources(),
            ctx(),
        );

        assert_eq!(lines.len(), 15, "one row per parameter and nothing else");
        assert!(lines.iter().all(|l| !l.starts_with(' ')), "{lines:#?}");
    }

    /// `presence_penalty` has no GGUF key, so gglib naming it can never be
    /// overriding a model author — however the metadata is spelled.
    #[test]
    fn a_field_no_model_can_reach_never_gains_a_note() {
        let resolved = InferenceConfig {
            presence_penalty: Some(1.5),
            ..InferenceConfig::with_hardcoded_defaults()
        };
        let lines = explanation_lines(
            &resolved,
            &auto_detected_sources(),
            ctx_publishing(&[("general.sampling.presence_penalty", "0.0")]),
        );

        assert_eq!(note_for(&lines, "presence_penalty"), None, "{lines:#?}");
    }

    /// **The regression guard for the bug this column had.** `{:<NAME_WIDTH$}`
    /// does not truncate an over-long name, it simply stops padding — so a
    /// name that outgrows the column collides with the value beside it and
    /// looks like a rendering glitch rather than a width bug. Both DRY names
    /// did exactly that for as long as they have existed.
    #[test]
    fn every_name_fits_its_column() {
        let lines = explanation_lines(
            &InferenceConfig::with_hardcoded_defaults(),
            &auto_detected_sources(),
            ctx(),
        );

        for line in lines.iter().filter(|l| !l.starts_with(' ')) {
            let name = line.split_whitespace().next().expect("a name");
            assert!(
                name.chars().count() < NAME_WIDTH,
                "'{name}' is {} chars and needs at least one space before its value,                  but NAME_WIDTH is {NAME_WIDTH}",
                name.chars().count()
            );
            // The value must be separated from the name, not butted against it.
            assert!(
                line.chars().nth(name.chars().count()) == Some(' '),
                "no gap after the name in: {line}"
            );
        }
    }

    /// Every note has to fit the separator the table is drawn with, or the
    /// alignment the rest of this module maintains is pointless.
    #[test]
    fn notes_fit_within_the_table_width() {
        let lines = explanation_lines(
            &InferenceConfig::with_hardcoded_defaults(),
            &auto_detected_sources(),
            ctx_publishing(&[
                ("general.sampling.temp", "0.33"),
                ("general.sampling.top_p", "0.71"),
                ("general.sampling.top_k", "17"),
                ("general.sampling.min_p", "0.011"),
                ("general.sampling.penalty_repeat", "1.07"),
            ]),
        );

        // Every reachable field published, so every one carries a note.
        assert_eq!(lines.iter().filter(|l| l.starts_with(' ')).count(), 5);
        for line in &lines {
            assert!(
                line.chars().count() + 2 <= SEP_WIDTH,
                "{} chars: {line}",
                line.chars().count()
            );
        }
    }

    /// Both caveats are always shown, and the client one reflects the setting.
    #[test]
    fn caveats_report_the_client_trust_setting() {
        assert!(caveats(ctx()).iter().any(|n| n.contains("is ignored")));

        let trusted = ExplainContext {
            trust_client_sampling: true,
            ..ctx()
        };
        assert!(caveats(trusted).iter().any(|n| n.contains("is trusted")));
    }

    /// The caveat that describes the trust boundary names all of it.
    ///
    /// This is the only place an operator is told what an untrusted client
    /// still gets, and for one release it was wrong: `reasoning_budget_tokens`
    /// joined `CLIENT_AUTHORITATIVE_KEYS` while the sentence went on saying
    /// "except max_tokens", because nothing connected the two. A prose
    /// description of a boundary is as much a part of the boundary as the
    /// `contains` call that enforces it.
    #[test]
    fn caveats_name_every_client_authoritative_key() {
        let notes = caveats(ctx());
        for key in CLIENT_AUTHORITATIVE_KEYS {
            assert!(
                notes.iter().any(|n| n.contains(key)),
                "{key} survives an untrusted request but no caveat says so: {notes:?}"
            );
        }
    }

    /// A caveat wider than the rule above it reads like an overflow.
    ///
    /// `notes_fit_within_the_table_width` covers the hanging notes under the
    /// rows and never covered these, which is how a derived caveat could have
    /// grown past the separator the moment a second key joined the carve-out.
    /// `print_explanation` indents by two, so that is the budget.
    #[test]
    fn caveats_fit_within_the_table_width() {
        let trusted = ExplainContext {
            trust_client_sampling: true,
            ..ctx()
        };
        for note in caveats(ctx()).iter().chain(caveats(trusted).iter()) {
            assert!(
                note.chars().count() + 2 <= SEP_WIDTH,
                "{} chars: {note}",
                note.chars().count()
            );
        }
    }
}
