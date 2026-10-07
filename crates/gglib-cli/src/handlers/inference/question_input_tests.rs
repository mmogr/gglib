//! The question `gglib q` ends on: whether to carry on into a chat.

use std::io;

use super::continue_from;
use crate::utils::input;

/// What the question is answered with when `typed` is all its input holds.
fn answer(typed: &str) -> bool {
    continue_from(&mut typed.as_bytes(), &mut io::sink()).expect("a string can be read")
}

/// Each row is what was typed and whether the chat goes on. A word is
/// followed by a line that says the opposite wherever one could decide
/// instead, so the word is what answered.
#[test]
fn enter_y_and_yes_carry_on_and_n_no_and_the_end_of_input_do_not() {
    let table = [
        ("\n", true),
        ("y\nn\n", true),
        ("Y\nn\n", true),
        ("yes\nn\n", true),
        ("YES\nn\n", true),
        ("n\ny\n", false),
        ("no\ny\n", false),
        ("NO\ny\n", false),
        ("", false),
    ];
    for (typed, carries_on) in table {
        assert_eq!(answer(typed), carries_on, "{typed:?}");
    }
}

/// A line that is none of the four words is asked again, and the line after
/// it answers: it neither declines nor carries on by itself.
#[test]
fn anything_else_is_asked_again() {
    assert!(answer("sure\ny\n"));
    assert!(!answer("yolo\nn\n"));
    assert!(answer("q\n\n"), "Enter still carries on once asked again");
    assert!(!answer("maybe\n"), "and the end of input still declines");
}

/// Whatever is typed, the answer is the one the CLI's shared question gives
/// with Enter for yes.
#[test]
fn the_answer_is_the_shared_questions() {
    let typed = [
        "",
        "\n",
        "y\n",
        "yes\n",
        "n\n",
        "no\n",
        " Y \r\n",
        "nope\nyes\n",
        "1\n0\n\n",
        "ok",
    ];
    for typed in typed {
        let shared = input::confirm_from(&mut typed.as_bytes(), &mut io::sink(), "?", true)
            .expect("a string can be read");
        assert_eq!(answer(typed), shared, "{typed:?}");
    }
}

/// The question is written to the writer it is given, which is stderr for
/// `q`, with the hint that Enter is yes.
#[test]
fn the_question_is_written_where_it_is_asked() {
    let mut asked_on = Vec::new();
    let mut entered: &[u8] = b"\n";

    continue_from(&mut entered, &mut asked_on).expect("a string can be read");

    assert_eq!(
        String::from_utf8(asked_on).expect("text"),
        "Continue chatting? (Y/n): \n"
    );
}
