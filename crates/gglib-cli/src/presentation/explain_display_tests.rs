//! Tests for [`super`] — the `gglib model explain` table.

use gglib_app_services::{ParamProvenanceDto, SuppressedEffortDto};
use gglib_core::domain::InferenceConfig;

use super::*;
use ProvenanceKindDto::{Floor, FloorCoupled, Layer, SuppressedByTemplate, Unset};
use SamplingLayerDto::{Global, ModelAutoDetected, ModelUserSet, Profile};

/// `resolved`, explained by [`auto_detected_sources`] for a model that
/// publishes nothing.
fn explained(resolved: InferenceConfig) -> SamplingExplanationDto {
    SamplingExplanationDto {
        resolved,
        sources: auto_detected_sources(),
        profile: None,
        is_reasoning: false,
        trust_client_sampling: false,
        published: Vec::new(),
        defaults_origin: None,
        effort_suppressed: None,
    }
}

/// The floor's own values under [`auto_detected_sources`].
fn plain() -> SamplingExplanationDto {
    explained(InferenceConfig::with_hardcoded_defaults())
}

/// `explanation`, with `param`'s row saying `kind` and `layer`.
fn sourced(
    mut explanation: SamplingExplanationDto,
    param: &str,
    kind: ProvenanceKindDto,
    layer: Option<SamplingLayerDto>,
) -> SamplingExplanationDto {
    let rows = &mut explanation.sources;
    let row = rows.iter_mut().find(|row| row.param == param).unwrap();
    (row.kind, row.layer) = (kind, layer);
    explanation
}

/// `explanation`, for a model whose GGUF publishes `key` for `param`, with
/// `state` being what gglib does about it.
fn publishing(
    mut explanation: SamplingExplanationDto,
    param: &str,
    key: &str,
    state: PublishedStateDto,
) -> SamplingExplanationDto {
    explanation.published.push(PublishedDefaultDto {
        param: param.to_owned(),
        key: key.to_owned(),
        state,
    });
    explanation
}

/// `explanation`, on a model whose template ignores the `level` that `layer`
/// asked for: what the gate leaves is no value, and the marker for a rung.
fn suppressing(
    explanation: SamplingExplanationDto,
    level: ReasoningEffort,
    layer: SamplingLayerDto,
) -> SamplingExplanationDto {
    let mut explanation = sourced(explanation, "reasoningEffort", SuppressedByTemplate, None);
    explanation.resolved.reasoning_effort = None;
    explanation.effort_suppressed = Some(SuppressedEffortDto {
        level,
        layer: Some(layer),
    });
    explanation
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
/// command exists to make legible. In the order the wire carries them.
fn auto_detected_sources() -> Vec<ParamProvenanceDto> {
    let auto = (Layer, Some(ModelAutoDetected));
    let (coupled, unset) = ((FloorCoupled, None), (Unset, None));
    let sources = [
        ("temperature", auto),
        ("topP", auto),
        ("topK", (Layer, Some(Global))),
        ("presencePenalty", auto),
        ("repeatPenalty", coupled),
        ("minP", coupled),
        ("frequencyPenalty", unset),
        ("dynatempRange", unset),
        ("dynatempExponent", unset),
        ("topNSigma", unset),
        ("dryMultiplier", coupled),
        ("dryBase", unset),
        ("dryAllowedLength", unset),
        ("dryPenaltyLastN", unset),
        ("maxTokens", unset),
        ("reasoningEffort", unset),
        ("reasoningBudgetTokens", unset),
    ];
    let row = |(param, (kind, layer)): (&str, _)| ParamProvenanceDto {
        param: param.to_owned(),
        kind,
        layer,
    };
    sources.into_iter().map(row).collect()
}

/// **No row may go quiet, and there is no longer a list of exceptions.**
///
/// `explanation_lines` pairs provenance with values by `zip`, which truncates
/// silently — a field added to the provenance and not to the value column
/// vanishes from this table with no compile error and no failing count. The two
/// reasoning controls spent an arc in a `DEFERRED_ROWS` register waiting for a
/// wider name column; this asserts the register is not needed, so the next
/// omission fails here rather than being added to it.
#[test]
fn every_provenance_row_is_rendered() {
    let lines = explanation_lines(&plain());

    for row in auto_detected_sources() {
        let named = |line: &String| {
            let name = line.split(' ').next().unwrap_or_default();
            name.replace('_', "") == row.param.to_lowercase()
        };
        assert!(
            lines.iter().any(named),
            "{} carries provenance and no row in {lines:#?}",
            row.param
        );
    }
}

#[test]
fn every_parameter_gets_exactly_one_line() {
    let lines = explanation_lines(&plain());
    assert_eq!(lines.len(), 17, "{lines:#?}");
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
        "reasoning_effort",
        "reasoning_budget_tokens",
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
    let user = describe(Layer, Some(ModelUserSet), &plain());
    let auto = describe(Layer, Some(ModelAutoDetected), &plain());

    assert!(user.contains("user-set"), "{user}");
    assert!(auto.contains("auto-detected"), "{auto}");
    assert_ne!(user, auto);
}

/// A floor reached because the coupling rule suppressed the layers below
/// must not look like one nobody ever set a value for.
#[test]
fn a_coupled_floor_says_why_it_is_the_floor() {
    let floor = describe(Floor, None, &plain());
    let coupled = describe(FloorCoupled, None, &plain());

    assert!(!floor.contains("coupled"), "{floor}");
    assert!(
        coupled.contains("coupled to temperature layer"),
        "{coupled}"
    );
}

/// A reasoning model sits on a different floor, and the value alone does
/// not say so.
#[test]
fn the_floor_is_named_so_the_two_are_distinguishable() {
    let reasoning = SamplingExplanationDto {
        is_reasoning: true,
        ..plain()
    };
    assert!(describe(Floor, None, &reasoning).contains("reasoning floor"));
    assert!(describe(Floor, None, &plain()).contains("default floor"));
}

/// `max_tokens` has no floor value on purpose; it must read as a decision
/// rather than as a missing number.
#[test]
fn an_absent_max_tokens_reads_as_deliberate() {
    assert_eq!(plain().resolved.max_tokens, None, "guards the premise");

    let lines = explanation_lines(&plain());
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
    let with_profile = SamplingExplanationDto {
        profile: Some("coding".to_owned()),
        ..plain()
    };
    assert_eq!(
        describe(Layer, Some(Profile), &with_profile),
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
    let lines = explanation_lines(&explained(resolved));

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
    let overridden = PublishedStateDto::Overridden {
        published: 0.33,
        sending: 1.0,
    };
    let lines = explanation_lines(&publishing(
        explained(resolved),
        "temperature",
        "general.sampling.temp",
        overridden,
    ));

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
    let lines = explanation_lines(&publishing(
        sourced(explained(resolved), "topK", Unset, None),
        "topK",
        "general.sampling.top_k",
        PublishedStateDto::Deferred { published: 17.0 },
    ));

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
    let lines = explanation_lines(&publishing(
        explained(resolved),
        "temperature",
        "general.sampling.temp",
        PublishedStateDto::Restated { published: 0.7 },
    ));

    let note = note_for(&lines, "temperature").expect("temperature carries a note");
    assert!(note.starts_with(MARK_INFO), "{note}");
    assert!(note.contains("the same value"), "{note}");
}

/// A value gglib cannot parse must read as unknown, never as an override —
/// gglib does not know what it displaced.
#[test]
fn an_unreadable_published_value_reads_as_unknown() {
    let lines = explanation_lines(&publishing(
        plain(),
        "temperature",
        "general.sampling.temp",
        PublishedStateDto::Unreadable,
    ));

    let note = note_for(&lines, "temperature").expect("temperature carries a note");
    assert!(note.starts_with(MARK_UNKNOWN), "{note}");
    assert!(note.contains("cannot read"), "{note}");
}

/// The ordinary model — nothing published — must render exactly as before.
/// A note on every row would train the reader to ignore all of them.
#[test]
fn a_model_that_publishes_nothing_gains_no_notes() {
    let lines = explanation_lines(&plain());

    assert_eq!(lines.len(), 17, "one row per parameter and nothing else");
    assert!(lines.iter().all(|l| !l.starts_with(' ')), "{lines:#?}");
}

/// `presence_penalty` has no GGUF key, so the explanation never carries an
/// entry for it, and a note hangs under the field it is about and no other.
#[test]
fn a_field_no_model_can_reach_never_gains_a_note() {
    let resolved = InferenceConfig {
        presence_penalty: Some(1.5),
        ..InferenceConfig::with_hardcoded_defaults()
    };
    let lines = explanation_lines(&publishing(
        explained(resolved),
        "temperature",
        "general.sampling.temp",
        PublishedStateDto::Restated { published: 0.7 },
    ));

    assert_eq!(note_for(&lines, "presence_penalty"), None, "{lines:#?}");
    assert_eq!(lines.iter().filter(|l| l.starts_with(' ')).count(), 1);
}

/// **The regression guard for the bug this column had.** `{:<NAME_WIDTH$}`
/// does not truncate an over-long name, it simply stops padding — so a
/// name that outgrows the column collides with the value beside it and
/// looks like a rendering glitch rather than a width bug. Both DRY names
/// did exactly that for as long as they have existed.
#[test]
fn every_name_fits_its_column() {
    let lines = explanation_lines(&plain());

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
    // The widest note, an override naming both numbers, on every key.
    let published = [
        ("temperature", "general.sampling.temp", 0.33, 0.7),
        ("topP", "general.sampling.top_p", 0.71, 0.95),
        ("topK", "general.sampling.top_k", 17.0, 40.0),
        ("minP", "general.sampling.min_p", 0.011, 0.05),
        (
            "repeatPenalty",
            "general.sampling.penalty_repeat",
            1.07,
            1.15,
        ),
    ];
    let explanation =
        published
            .into_iter()
            .fold(plain(), |explanation, (param, key, published, sending)| {
                let state = PublishedStateDto::Overridden { published, sending };
                publishing(explanation, param, key, state)
            });
    let lines = explanation_lines(&explanation);

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
    assert!(caveats(&plain()).iter().any(|n| n.contains("is ignored")));

    let trusted = SamplingExplanationDto {
        trust_client_sampling: true,
        ..plain()
    };
    assert!(caveats(&trusted).iter().any(|n| n.contains("is trusted")));
}

/// The caveat that describes the trust boundary names all of it.
///
/// This is the only place an operator is told what an untrusted client
/// still gets, and for one release it was wrong: `reasoning_budget_tokens`
/// joined `CLIENT_AUTHORITATIVE_KEYS` while the sentence went on saying
/// "except `max_tokens`", because nothing connected the two. A prose
/// description of a boundary is as much a part of the boundary as the
/// `contains` call that enforces it.
#[test]
fn caveats_name_every_client_authoritative_key() {
    let notes = caveats(&plain());
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
    let trusted = SamplingExplanationDto {
        trust_client_sampling: true,
        ..plain()
    };
    for note in caveats(&plain()).iter().chain(caveats(&trusted).iter()) {
        assert!(
            note.chars().count() + 2 <= SEP_WIDTH,
            "{} chars: {note}",
            note.chars().count()
        );
    }
}

// =============================================================================
// The reasoning controls
// =============================================================================

/// A resolution where the `high` profile named an effort and a budget.
fn reasoning() -> SamplingExplanationDto {
    let resolved = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(16384),
        ..InferenceConfig::with_hardcoded_defaults()
    };
    let named = sourced(explained(resolved), "reasoningEffort", Layer, Some(Profile));
    SamplingExplanationDto {
        profile: Some("high".to_owned()),
        ..sourced(named, "reasoningBudgetTokens", Layer, Some(Profile))
    }
}

/// The row that could not be drawn before this table's name column grew.
#[test]
fn both_reasoning_controls_render_their_value_and_their_rung() {
    let lines = explanation_lines(&reasoning());

    let effort = lines
        .iter()
        .find(|l| l.starts_with("reasoning_effort"))
        .expect("reasoning_effort is rendered");
    assert!(effort.contains("high"), "{effort}");
    assert!(effort.contains("profile 'high'"), "{effort}");

    let budget = lines
        .iter()
        .find(|l| l.starts_with("reasoning_budget_tokens"))
        .expect("reasoning_budget_tokens is rendered");
    assert!(budget.contains("16384"), "{budget}");
    assert!(budget.contains("profile 'high'"), "{budget}");
}

/// The level renders as its own word, not as a `Debug` spelling of the enum.
#[test]
fn a_level_renders_as_its_wire_name() {
    assert_eq!(fmt_effort(Some(ReasoningEffort::XHigh)), "xhigh");
    assert_eq!(fmt_effort(Some(ReasoningEffort::Minimal)), "minimal");
    assert_eq!(fmt_effort(None), ABSENT);
}

/// `-1` is the defer sentinel and must survive to the column as a number
/// rather than being read as a missing value.
#[test]
fn the_defer_sentinel_is_a_value_not_an_absence() {
    let mut deferring = reasoning();
    deferring.resolved.reasoning_budget_tokens = Some(-1);
    let line = explanation_lines(&deferring)
        .into_iter()
        .find(|l| l.starts_with("reasoning_budget_tokens"))
        .expect("rendered");

    assert!(line.contains("-1"), "{line}");
    assert!(!line.contains(ABSENT), "{line}");
}

/// **The headline case, and the one a bare `unset` would have hidden.** A
/// `:high` profile on a model whose template ignores the variable is inert, and
/// the value alone cannot say so — the row would read exactly like a model
/// nobody configured.
#[test]
fn a_suppressed_effort_reads_differently_from_one_nobody_set() {
    let suppressed = suppressing(reasoning(), ReasoningEffort::High, Profile);

    let lines = explanation_lines(&suppressed);
    let row = lines
        .iter()
        .find(|l| l.starts_with("reasoning_effort"))
        .expect("rendered");

    assert!(row.contains("suppressed"), "{row}");
    assert!(row.contains("template"), "{row}");

    let unset = explanation_lines(&plain());
    let never_set = unset
        .iter()
        .find(|l| l.starts_with("reasoning_effort"))
        .expect("rendered");
    assert!(never_set.contains("unset by design"), "{never_set}");
    assert!(!never_set.contains("suppressed"), "{never_set}");
}

/// **The actionable half.** Knowing a level was suppressed does not tell an
/// operator what to change; knowing it was *the `:high` profile's* level does.
/// Both facts are destroyed by the gate — it clears the value and overwrites
/// the rung — so they reach the table on the explanation or not at all.
#[test]
fn the_suppression_note_names_the_level_and_the_rung() {
    let suppressed = suppressing(reasoning(), ReasoningEffort::High, Profile);
    let lines = explanation_lines(&suppressed);

    let note = note_for(&lines, "reasoning_effort").expect("carries a note");
    assert_eq!(note, "! 'high' from profile 'high'; not sent");
}

/// This command explains stored configuration and has sent nothing, so nothing
/// it prints may report a request. The row's wording is a standing property of
/// the model — true of every request against it — and the note is a label, not
/// a claim about one that ran.
#[test]
fn nothing_in_the_suppression_claims_a_request_happened() {
    let suppressed = suppressing(plain(), ReasoningEffort::Max, Global);
    let row = describe(SuppressedByTemplate, None, &suppressed);
    let note = suppression_note("reasoning_effort", &suppressed).expect("a note");

    assert!(
        row.contains("this model's template"),
        "the row states a property of the model: {row}"
    );
    for claim in ["was ", "did not", "we sent", "gglib sent"] {
        assert!(!row.contains(claim), "{claim} in {row}");
        assert!(!note.contains(claim), "{claim} in {note}");
    }
    assert!(note.contains("not sent"), "{note}");
}

/// The note belongs to one row. Hanging it under `max_tokens` would attach a
/// reasoning fact to a budget nobody suppressed.
#[test]
fn only_the_effort_row_carries_the_suppression_note() {
    let suppressed = suppressing(reasoning(), ReasoningEffort::High, Profile);
    for field in ["temperature", "max_tokens", "reasoning_budget_tokens"] {
        assert_eq!(suppression_note(field, &suppressed), None, "{field}");
    }
}

/// A model whose template *does* read the variable, or has never been probed,
/// must gain no note at all — the overwhelming majority of rows.
#[test]
fn an_unsuppressed_effort_gains_no_note() {
    let lines = explanation_lines(&reasoning());
    assert_eq!(note_for(&lines, "reasoning_effort"), None, "{lines:#?}");
}

/// The widest row this table can draw, checked against its own rule. The
/// auto-detected label is the longest source string and `reasoning_budget_tokens`
/// the longest name, so this is the corner both constants were sized for.
#[test]
fn the_reasoning_rows_and_their_note_fit_the_table_width() {
    let resolved = InferenceConfig {
        reasoning_budget_tokens: Some(32768),
        ..InferenceConfig::with_hardcoded_defaults()
    };
    // The rung with the longest label: "per-model defaults (auto-detected:
    // reasoning tag)".
    let longest = Some(ModelAutoDetected);
    let budgeted = sourced(explained(resolved), "reasoningBudgetTokens", Layer, longest);
    let lines = explanation_lines(&suppressing(
        budgeted,
        ReasoningEffort::Minimal,
        ModelAutoDetected,
    ));

    assert_eq!(lines.len(), 18, "seventeen rows and the note: {lines:#?}");
    for line in &lines {
        assert!(
            line.chars().count() + 2 <= SEP_WIDTH,
            "{} chars: {line}",
            line.chars().count()
        );
    }
}
