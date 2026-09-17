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

/// The question ADR 0011's first criterion asks: of the guard's trips, which
/// were stagnation? A model that only stagnated says so, and does not print a
/// loop-detector row at zero beside it.
#[test]
fn a_trip_is_shown_under_the_detector_that_raised_it() {
    let stagnant = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.loop_guard_trips = 3;
            c.loop_guard_stagnations = 3;
        }),
    )]);
    let rendered = render_defects_section(&stagnant);
    assert!(
        rendered.contains("loop-guard trips         3"),
        "{rendered}"
    );
    assert!(rendered.contains("stagnation detector"), "{rendered}");
    assert!(!rendered.contains("loop detector"), "{rendered}");

    let both = BTreeMap::from([(
        "qwen".to_string(),
        counts(|c| {
            c.loop_guard_trips = 3;
            c.loop_guard_loops = 1;
            c.loop_guard_stagnations = 2;
        }),
    )]);
    let rendered = render_defects_section(&both);
    assert!(
        rendered.contains("      loop detector            1"),
        "{rendered}"
    );
    assert!(
        rendered.contains("      stagnation detector      2"),
        "{rendered}"
    );
}

/// A proxy that predates the split sends the sum and neither part. The sum
/// still prints, alone, rather than beside two zeroes it never measured.
#[test]
fn an_older_proxys_trips_print_as_the_sum_alone() {
    let per_model = BTreeMap::from([("qwen".to_string(), counts(|c| c.loop_guard_trips = 2))]);
    let rendered = render_defects_section(&per_model);
    assert!(rendered.contains("loop-guard trips"), "{rendered}");
    assert!(!rendered.contains("detector"), "{rendered}");
}

/// The same older proxy, through serde this time rather than the helper above:
/// the frame it sends has no key for either part. Each new field defaults, so
/// the frame still deserialises and the parts read zero; without the default,
/// one missing key would fail the whole dashboard.
#[test]
fn an_older_proxys_frame_still_deserialises_without_the_parts() {
    let old: ModelDefectCounts = serde_json::from_str(r#"{"requests":9,"loop_guard_trips":5}"#)
        .expect("a frame that predates the split still reads");
    assert_eq!(old.loop_guard_trips, 5);
    assert_eq!(
        (old.loop_guard_loops, old.loop_guard_stagnations),
        (0, 0),
        "the parts an older proxy never sent read as zero"
    );
}

/// The mirror in `wire` is kept in step with the proxy's struct by hand, and
/// every field of it defaults, so a misspelt one would read zero for ever and
/// fail nothing. This reads the proxy's own serialisation through it, every
/// field carrying a value no other field has.
#[test]
fn the_proxys_own_counts_are_read_through_the_mirror() {
    let sent = gglib_core::domain::defects::ModelDefectCounts {
        requests: 1,
        loop_guard_trips: 2,
        loop_guard_loops: 3,
        loop_guard_stagnations: 4,
        repairs_attempted: 5,
        repairs_succeeded: 6,
        stream_errors: 7,
        truncated_generations: 8,
        empty_responses: 9,
        reasoning_only: 10,
        dialect_residue: 11,
        unvalidatable_schemas: 12,
        normalization_errors: 13,
        identical_result_repeats: 14,
        repeats_not_evaluated: 15,
        repeats_rescued: 16,
    };
    let json = serde_json::to_string(&sent).expect("the proxy's struct serialises");
    let got: ModelDefectCounts = serde_json::from_str(&json).expect("the mirror reads it");

    let read = [
        got.requests,
        got.loop_guard_trips,
        got.loop_guard_loops,
        got.loop_guard_stagnations,
        got.repairs_attempted,
        got.repairs_succeeded,
        got.stream_errors,
        got.truncated_generations,
        got.empty_responses,
        got.reasoning_only,
        got.dialect_residue,
        got.unvalidatable_schemas,
        got.normalization_errors,
        got.identical_result_repeats,
        got.repeats_not_evaluated,
        got.repeats_rescued,
    ];
    assert_eq!(read, std::array::from_fn(|i| i as u64 + 1), "{json}");
}
