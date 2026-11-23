//! The step limit `chat` and `q` take from `--max-tool-iterations`.

use gglib_core::MAX_ITERATIONS_CEILING;

use super::step_limit;

/// What `step_limit` returns for `asked`, and what it said.
fn limit(asked: Option<usize>) -> (Option<usize>, String) {
    let mut said = Vec::new();
    let used = step_limit(asked, &mut said).expect("a Vec takes the line");
    (used, String::from_utf8(said).expect("the line is UTF-8"))
}

/// No flag, a value under the ceiling and the ceiling itself run as asked,
/// and nothing is said.
#[test]
fn a_limit_at_or_under_the_ceiling_runs_as_asked_and_says_nothing() {
    for asked in [None, Some(1), Some(7), Some(MAX_ITERATIONS_CEILING)] {
        assert_eq!(limit(asked), (asked, String::new()), "{asked:?}");
    }
}

/// A value above the ceiling runs with the ceiling, and one line names the
/// value asked and the ceiling used.
#[test]
fn a_limit_above_the_ceiling_runs_at_the_ceiling_and_says_so() {
    let ceiling = MAX_ITERATIONS_CEILING;
    for asked in [ceiling + 1, ceiling * 2, usize::MAX] {
        let (used, said) = limit(Some(asked));
        assert_eq!(used, Some(ceiling), "{asked}");
        assert_eq!(
            said,
            format!(
                "--max-tool-iterations {asked} is more than the {ceiling} steps a run may take; \
                 it runs with {ceiling}.\n"
            ),
        );
    }
}
