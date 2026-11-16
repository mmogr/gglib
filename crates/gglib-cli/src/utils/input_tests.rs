//! Tests for the rule every question is asked by, and for the yes/no
//! question every confirming command asks through.

use super::*;

/// What the question is answered with when `typed` is all its input holds.
fn answer(typed: &str, default: bool) -> bool {
    confirm_from(&mut typed.as_bytes(), &mut io::sink(), "Proceed?", default)
        .expect("a string can be read")
}

const DEFAULTS: [bool; 2] = [false, true];

/// Each word is followed by a line that says the opposite, so the word is
/// what answered: one that was not taken for an answer would be asked again
/// and the next line would decide.
#[test]
fn y_and_yes_are_yes_and_n_and_no_are_no_in_any_case_whatever_the_default() {
    for default in DEFAULTS {
        for yes in ["y", "Y", "yes", "YES", "Yes", "  y ", "yes\r"] {
            let typed = format!("{yes}\nn\n");
            assert!(answer(&typed, default), "{typed:?}, default {default}");
        }
        for no in ["n", "N", "no", "NO", "No", "  n ", "no\r"] {
            let typed = format!("{no}\ny\n");
            assert!(!answer(&typed, default), "{typed:?}, default {default}");
        }
    }
}

#[test]
fn an_empty_line_takes_the_default() {
    for default in DEFAULTS {
        for enter in ["\n", "\r\n", "   \n"] {
            assert_eq!(answer(enter, default), default, "{enter:?}");
        }
    }
}

/// Nobody typing is nobody agreeing: a closed or exhausted input answers no
/// even where Enter would have answered yes.
#[test]
fn the_end_of_input_is_no_whatever_the_default() {
    for default in DEFAULTS {
        assert!(!answer("", default), "default {default}");
        assert!(!answer("maybe\n", default), "default {default}");
    }
}

/// A last line with no newline after it was still typed.
#[test]
fn an_answer_with_no_newline_after_it_is_still_the_answer() {
    for default in DEFAULTS {
        assert!(answer("yes", default), "default {default}");
        assert!(!answer("n", default), "default {default}");
    }
}

/// Only the four words answer. A word that merely starts with `y` is not a
/// yes, and one that is not `n` is not a yes either: each is asked again, and
/// the line after it decides.
#[test]
fn anything_else_is_asked_again_and_the_next_line_answers() {
    for default in DEFAULTS {
        assert!(!answer("yolo\nn\ny\n", default), "default {default}");
        assert!(!answer("yeah\nno\ny\n", default), "default {default}");
        assert!(answer("nope\nyes\n", default), "default {default}");
        assert!(answer("0\nfalse\ny\n", default), "default {default}");
        assert_eq!(answer("what\n\n", default), default);
    }
}

/// One question reads one answer and leaves the rest, so a command that asks
/// twice gets each question its own line.
#[test]
fn a_question_reads_only_its_own_answer() {
    let mut typed: &[u8] = b"y\nn\nleft over\n";

    let asked_on = &mut io::sink();
    let first = confirm_from(&mut typed, asked_on, "Continue?", false).expect("read");
    let second = confirm_from(&mut typed, asked_on, "Apply?", true).expect("read");

    assert!(first);
    assert!(!second);
    assert_eq!(typed, b"left over\n");
}

/// The question is written where the caller says, with the hint that shows
/// which way Enter goes, and written again for each line that did not answer.
#[test]
fn the_question_is_written_where_asked_once_for_each_line_read() {
    let mut asked_on = Vec::new();
    let (mut asked_twice, mut entered): (&[u8], &[u8]) = (b"what\ny\n", b"\n");

    confirm_from(&mut asked_twice, &mut asked_on, "Proceed?", false).expect("read");
    confirm_from(&mut entered, &mut asked_on, "Carry on?", true).expect("read");

    assert_eq!(
        String::from_utf8(asked_on).expect("text"),
        "Proceed? (y/N): \nProceed? (y/N): \nCarry on? (Y/n): \n"
    );
}

// ── A question is asked only of a terminal ──────────────────────────────────

/// What a refused question says in these tests.
const REFUSED: &str = "There is no terminal to ask, so nothing was done: run it again with --yes.";

/// What a taken answer says in these tests.
const TAKEN: &str = "Took the stated answer (no terminal to ask).";

/// A yes/no question asked from `from`, Enter being yes, with `typed` all
/// its input holds: what it came to, what it wrote, and what it left unread.
fn yes_no(from: Input, typed: &str, unasked: Unasked<bool>) -> (Result<bool>, String, String) {
    let mut input = typed.as_bytes();
    let mut asked_on = Vec::new();
    let answer = yes_no_from(from, &mut input, &mut asked_on, "Proceed?", true, unasked);
    let unread = String::from_utf8(input.to_vec()).expect("text");
    (answer, String::from_utf8(asked_on).expect("text"), unread)
}

/// [`yes_no`] for a number question, the answer as it prints.
fn number(from: Input, typed: &str, unasked: Unasked<f64>) -> (Result<String>, String, String) {
    let mut input = typed.as_bytes();
    let mut asked_on = Vec::new();
    let answer = number_from(from, &mut input, &mut asked_on, "Count", unasked)
        .map(|answer| answer.to_string());
    let unread = String::from_utf8(input.to_vec()).expect("text");
    (answer, String::from_utf8(asked_on).expect("text"), unread)
}

/// With no terminal, a question with no answer to take is refused with its
/// sentence, and its input is not read: not a yes waiting in a pipe, not
/// the end of one, and so not a pipe that never ends, which would wait.
#[test]
fn with_no_terminal_a_question_with_nothing_to_take_is_refused_unread() {
    for typed in ["y\n", "", "\n"] {
        let (answer, asked_on, unread) = yes_no(
            Input::NotTerminal,
            typed,
            Unasked::Refuse(REFUSED.to_owned()),
        );

        let refused = answer.expect_err("refused");
        assert_eq!(refused.to_string(), REFUSED, "{typed:?}");
        assert_eq!(asked_on, "", "nothing was asked: {typed:?}");
        assert_eq!(unread, typed, "nothing was read");
    }

    let (answer, asked_on, unread) = number(
        Input::NotTerminal,
        "9\n",
        Unasked::Refuse(REFUSED.to_owned()),
    );
    assert_eq!(answer.expect_err("refused").to_string(), REFUSED);
    assert_eq!((asked_on.as_str(), unread.as_str()), ("", "9\n"));
}

/// With no terminal, the answer a question can state is taken, the line
/// saying so is written where the question would have been, and the input
/// is not read.
#[test]
fn with_no_terminal_the_stated_answer_is_taken_and_said() {
    let (answer, asked_on, unread) = number(
        Input::NotTerminal,
        "9\n",
        Unasked::Take(4.0, TAKEN.to_owned()),
    );
    assert_eq!(answer.expect("taken"), "4");
    assert_eq!(asked_on, format!("{TAKEN}\n"));
    assert_eq!(unread, "9\n");

    let (answer, asked_on, unread) = yes_no(
        Input::NotTerminal,
        "y\n",
        Unasked::Take(false, TAKEN.to_owned()),
    );
    assert!(!answer.expect("taken"));
    assert_eq!(asked_on, format!("{TAKEN}\n"));
    assert_eq!(unread, "y\n");
}

/// The rule is in one place: with no terminal, the question is never put.
#[test]
fn with_no_terminal_the_question_is_never_put() {
    let mut put = false;
    let answer = asked(
        Input::NotTerminal,
        &mut io::sink(),
        Unasked::Take(1, String::new()),
        |_| {
            put = true;
            Ok(2)
        },
    );

    assert_eq!(answer.expect("taken"), 1);
    assert!(!put);
}

/// A terminal is asked as it always was: the question with its hint, an
/// answer read, Enter for the answer shown, and anything else asked again.
#[test]
fn a_terminal_is_asked_and_its_answer_read() {
    fn refuse<T>() -> Unasked<T> {
        Unasked::Refuse(REFUSED.to_owned())
    }

    let (answer, asked_on, unread) = yes_no(Input::Terminal, "n\nleft\n", refuse());
    assert!(!answer.expect("read"));
    assert_eq!(
        (asked_on.as_str(), unread.as_str()),
        ("Proceed? (Y/n): \n", "left\n")
    );

    let (answer, asked_on, _) = number(Input::Terminal, "\n", Unasked::Take(4.0, TAKEN.to_owned()));
    assert_eq!(answer.expect("read"), "4");
    assert_eq!(asked_on, "Count [4.0]: \n", "the line is for nobody to ask");

    let (answer, asked_on, _) = number(Input::Terminal, "abc\n-1\n7.5\n", refuse());
    assert_eq!(answer.expect("read"), "7.5");
    assert_eq!(asked_on, "Count: \nCount: \nCount: \n");
}

/// At a terminal, the end of input (Ctrl+D) is a no to a yes/no question,
/// and to a number the answer Enter takes.
#[test]
fn at_a_terminal_the_end_of_input_is_no_or_the_stated_answer() {
    let (answer, _, _) = yes_no(Input::Terminal, "", Unasked::Take(true, TAKEN.to_owned()));
    assert!(!answer.expect("read"));

    let (answer, asked_on, _) = number(Input::Terminal, "", Unasked::Take(4.0, TAKEN.to_owned()));
    assert_eq!(answer.expect("read"), "4");
    assert_eq!(asked_on, "Count [4.0]: \n");
}

/// The sentence a question nobody can answer is refused with.
#[test]
fn the_refusal_says_what_was_not_done_and_the_flag_that_does_it() {
    assert_eq!(
        nobody_to_ask("the model was not removed", "--yes to remove it"),
        "There is no terminal to ask, so the model was not removed: run it again with --yes to \
         remove it."
    );
}
