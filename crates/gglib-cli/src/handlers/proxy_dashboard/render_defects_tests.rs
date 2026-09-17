//! Tests for [`super`] — the per-model signals section.
//!
//! Moved here with the section they test. Nearly all of them assert an absence:
//! a row that must not print, a model that must not earn a block, a "none" the
//! run has not earned yet.

use super::*;

fn counts(build: impl FnOnce(&mut ModelDefectCounts)) -> ModelDefectCounts {
    let mut c = ModelDefectCounts {
        requests: 100,
        ..Default::default()
    };
    build(&mut c);
    c
}

/// Before anything has been forwarded there is no evidence either way,
/// and "none" would be a claim this run has not earned.
#[test]
fn defects_distinguish_no_evidence_from_a_clean_run() {
    let empty = render_defects_section(&BTreeMap::new());
    assert!(empty.contains("nothing recorded yet"), "{empty}");

    let clean = BTreeMap::from([("qwen".to_string(), counts(|_| {}))]);
    let rendered = render_defects_section(&clean);
    assert!(
        rendered.contains("none across 100 request(s)"),
        "{rendered}"
    );
}

/// A healthy model produces zero of every counter, so listing it would
/// bury the one model that has something wrong with it.
#[test]
fn only_models_with_something_to_report_get_a_line() {
    let per_model = BTreeMap::from([
        ("healthy-model".to_string(), counts(|_| {})),
        ("sick-model".to_string(), counts(|c| c.stream_errors = 3)),
    ]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("sick-model"), "{rendered}");
    assert!(!rendered.contains("healthy-model"), "{rendered}");
    assert!(rendered.contains("stream errors"), "{rendered}");
}

/// The ratio is the number worth watching: attempts say how often this
/// model packages a call badly, successes whether the one lever works.
#[test]
fn repairs_are_shown_as_a_ratio() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.repairs_attempted = 9;
            c.repairs_succeeded = 7;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("7 of 9 succeeded"), "{rendered}");
}

/// `reasoning_only` is counted *within* `empty_responses`, not beside it.
/// Printing them as peers reads as more empty turns than happened.
#[test]
fn reasoning_only_turns_are_shown_inside_the_empty_total() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.empty_responses = 4;
            c.reasoning_only = 3;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(
        rendered.contains("empty responses          4 (3 reasoning-only)"),
        "{rendered}"
    );
}

/// A model whose only signal is repeated calls returning identical results
/// has failed at nothing gglib measures, and is still the model this
/// dashboard most needs to show.
#[test]
fn identical_result_repeats_alone_still_lists_the_model() {
    let per_model = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| c.identical_result_repeats = 3),
    )]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("qwen"), "{rendered}");
    assert!(rendered.contains("observed"), "{rendered}");
    assert!(rendered.contains("repeated, same result"), "{rendered}");
    assert!(rendered.contains('3'), "{rendered}");
}

/// A fleet whose joins never succeed must not read as a clean one — that is
/// the reading the second counter exists to prevent anyone acting on.
#[test]
fn repeats_that_could_not_be_evaluated_still_list_the_model() {
    let per_model = BTreeMap::from([("qwen".to_string(), counts(|c| c.repeats_not_evaluated = 9))]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("qwen"), "{rendered}");
    assert!(rendered.contains("repeated, not comparable"), "{rendered}");
    assert!(!rendered.contains("repeated, same result"), "{rendered}");
}

/// The reading ADR 0010's kill criteria rest on. A fleet whose repeats are all
/// rescued by a moving answer is one where the guard has effectively stopped
/// guarding, and neither counter beside this one can show that.
#[test]
fn rescued_repeats_alone_still_list_the_model() {
    let per_model = BTreeMap::from([("qwen".to_string(), counts(|c| c.repeats_rescued = 41))]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("qwen"), "{rendered}");
    assert!(rendered.contains("observed"), "{rendered}");
    assert!(rendered.contains("repeated, new result"), "{rendered}");
    assert!(rendered.contains("41"), "{rendered}");
    assert!(!rendered.contains("repeated, same result"), "{rendered}");
}

/// Every row must fit an 80-column terminal: the observed block sits at a
/// deeper indent than the defect rows and must not append prose past the
/// label column.
#[test]
fn observed_rows_fit_an_eighty_column_terminal() {
    let per_model = BTreeMap::from([(
        "a-model-with-a-fairly-long-name".to_string(),
        counts(|c| {
            c.identical_result_repeats = 123_456;
            c.repeats_not_evaluated = 123_456;
            c.repeats_rescued = 123_456;
        }),
    )]);

    let rendered = render_defects_section(&per_model);
    for line in rendered.lines() {
        assert!(
            line.chars().count() <= 80,
            "line exceeds 80 columns ({}): {line:?}",
            line.chars().count()
        );
    }
}

/// A counter at zero is not news. Only what fired is printed, or a model
/// with one bad turn costs eight lines of zeroes.
#[test]
fn a_counter_that_never_fired_is_not_printed() {
    let per_model = BTreeMap::from([("qwen".to_string(), counts(|c| c.dialect_residue = 1))]);

    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("dialect residue"), "{rendered}");
    assert!(!rendered.contains("loop-guard trips"), "{rendered}");
    assert!(!rendered.contains("truncated at ceiling"), "{rendered}");
}
