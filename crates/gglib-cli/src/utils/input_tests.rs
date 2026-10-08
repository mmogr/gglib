//! Tests for the yes/no question every confirming command asks through.

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
