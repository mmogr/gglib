//! Renders a resolved sampling config alongside the layer that supplied each
//! parameter, for `gglib model explain`.
//!
//! Format-only, like the rest of [`crate::presentation`]: the resolution
//! itself happens in `gglib-core`, and nothing here re-derives a value or a
//! source. Plain text with no colour, matching [`super::inspect_display`] —
//! a fact about a model is not a state, so it borrows no state colour.

use gglib_core::domain::{FieldSources, InferenceConfig, ParamSource, SamplingLayer};

use super::tables::print_separator;

/// Width of the parameter-name column. The longest name is
/// `presence_penalty` at 16 characters.
const NAME_WIDTH: usize = 17;

/// Width of the value column, wide enough for `-0.0000` style floats without
/// pushing the source column into a second screen.
const VALUE_WIDTH: usize = 7;

/// Wide enough to span the longest row: the two-space indent, both columns,
/// and the longest source label (`per-model defaults (auto-detected: reasoning
/// tag)`).
const SEP_WIDTH: usize = 78;

/// The arrow separating a value from its provenance.
const ARROW: &str = "\u{2190}";

/// Shown for a parameter that resolved to no value at all.
const ABSENT: &str = "\u{2014}";

/// How the model's own defaults should be described, once resolved.
///
/// The wording matches `inspect_display`'s, so the two commands describe the
/// same stored fact the same way.
#[derive(Debug, Clone, Copy)]
pub struct ExplainContext<'a> {
    /// The profile that was selected, if any.
    pub profile: Option<&'a str>,
    /// Whether the model carries the `reasoning` tag, which selects the floor.
    pub is_reasoning: bool,
    /// Whether the client's own sampling parameters are honoured at request
    /// time. Shown as a caveat, since this command explains stored
    /// configuration and cannot see a live request.
    pub trust_client_sampling: bool,
}

/// Print the resolved parameters and their provenance.
pub fn print_explanation(
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
#[must_use]
pub fn explanation_lines(
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
        ("max_tokens", fmt_u32(resolved.max_tokens)),
    ];

    sources
        .iter()
        .zip(values)
        .map(|((field, source), (value_field, value))| {
            debug_assert_eq!(
                field, value_field,
                "provenance and value rows must stay aligned"
            );
            format!(
                "{field:<NAME_WIDTH$}{value:<VALUE_WIDTH$} {ARROW} {}",
                describe(source, ctx)
            )
        })
        .collect()
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
            Some(SamplingLayer::ModelAutoDetected) => {
                "per-model defaults (auto-detected: reasoning tag)".to_owned()
            }
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
fn caveats(ctx: ExplainContext<'_>) -> Vec<String> {
    let mut notes = vec![
        "Operator flags (gglib proxy --temperature, ...) outrank every layer above.".to_owned(),
    ];
    notes.push(if ctx.trust_client_sampling {
        "Client-supplied sampling is trusted and outranks all but those flags.".to_owned()
    } else {
        "Client-supplied sampling is ignored, except max_tokens.".to_owned()
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
        }
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
            max_tokens: ParamSource::Unset,
        }
    }

    #[test]
    fn every_parameter_gets_exactly_one_line() {
        let lines = explanation_lines(
            &InferenceConfig::with_hardcoded_defaults(),
            &auto_detected_sources(),
            ctx(),
        );
        assert_eq!(lines.len(), 7, "{lines:#?}");
        for field in [
            "temperature",
            "top_p",
            "top_k",
            "presence_penalty",
            "repeat_penalty",
            "min_p",
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
}
