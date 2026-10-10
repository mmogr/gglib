//! What each change writes: a saved reply is never discarded or altered.

use super::fixture::{greeted, image, kyoto, unanswered};
use super::{ChatChange, Plan, Refused, Then, answerable, plan};

fn edit(message_id: i64, content: &str) -> ChatChange {
    ChatChange::Edit {
        message_id,
        content: content.to_owned(),
        images: Vec::new(),
    }
}

fn fork(through: Option<i64>, then: Then, answer: bool) -> Plan {
    Plan::Fork {
        through,
        then,
        answer,
    }
}

/// The question's reply, and everything after it, stay in the chat it was
/// asked in.
#[test]
fn an_edit_of_an_answered_question_branches_before_it_and_asks_again() {
    let path = kyoto();
    assert_eq!(
        plan(&path, &edit(3, "Make it longer"), false),
        Ok(fork(Some(2), Then::Question, true))
    );
    assert_eq!(
        plan(&path, &edit(1, "Plan a trip to Osaka"), false),
        Ok(fork(None, Then::Question, true))
    );
}

#[test]
fn an_edit_of_the_last_question_nothing_answers_replaces_it() {
    let path = unanswered();
    let change = ChatChange::Edit {
        message_id: 7,
        content: "And with kids?".to_owned(),
        images: Vec::new(),
    };
    assert_eq!(
        plan(&path, &change, false),
        Ok(Plan::Replace { question: 7 })
    );
}

/// A reply is being written to the chat: the question it answers is not
/// replaced under it.
#[test]
fn an_edit_of_the_last_question_while_a_reply_is_written_branches() {
    let path = unanswered();
    assert_eq!(
        plan(&path, &edit(7, "And with a dog?"), true),
        Ok(fork(Some(6), Then::Question, true))
    );
}

#[test]
fn an_edit_that_changes_nothing_is_refused() {
    let path = unanswered();
    let same = ChatChange::Edit {
        message_id: 7,
        content: "And with kids?".to_owned(),
        images: vec![image(0)],
    };
    assert_eq!(plan(&path, &same, false), Err(Refused::Unchanged));
    assert_eq!(
        plan(&path, &edit(2, "Day 1: temples\nDay 2: markets"), false),
        Err(Refused::Unchanged)
    );
}

/// The reply's tool calls and their results go with it: the edited reply is
/// one message of plain text.
#[test]
fn an_edit_of_any_message_of_a_reply_branches_with_the_edited_reply() {
    let path = kyoto();
    for id in [4, 5, 6] {
        assert_eq!(
            plan(&path, &edit(id, "One day: Kinkaku-ji."), false),
            Ok(fork(Some(3), Then::EditedReply, false)),
            "message {id}"
        );
    }
}

#[test]
fn an_edited_reply_carries_no_image() {
    let path = kyoto();
    let change = ChatChange::Edit {
        message_id: 2,
        content: "Day 1: gardens".to_owned(),
        images: vec![image(0)],
    };
    assert_eq!(plan(&path, &change, false), Err(Refused::ImagesOnReply));
}

#[test]
fn a_regenerate_branches_after_the_question_and_answers_it_again() {
    let path = kyoto();
    assert_eq!(
        plan(&path, &ChatChange::Regenerate { message_id: 5 }, false),
        Ok(fork(Some(3), Then::Nothing, true))
    );
    assert_eq!(
        plan(&path, &ChatChange::Regenerate { message_id: 2 }, false),
        Ok(fork(Some(1), Then::Nothing, true))
    );
}

#[test]
fn a_question_is_not_regenerated_and_a_greeting_answers_nothing() {
    assert_eq!(
        plan(&kyoto(), &ChatChange::Regenerate { message_id: 3 }, false),
        Err(Refused::NotAReply(3))
    );
    assert_eq!(
        plan(&greeted(), &ChatChange::Regenerate { message_id: 1 }, false),
        Err(Refused::NothingToAnswer)
    );
}

#[test]
fn a_branch_copies_as_far_as_the_end_of_the_turn() {
    let path = kyoto();
    assert_eq!(
        plan(&path, &ChatChange::Branch { message_id: 4 }, false),
        Ok(fork(Some(6), Then::Nothing, false))
    );
    assert_eq!(
        plan(&path, &ChatChange::Branch { message_id: 3 }, false),
        Ok(fork(Some(3), Then::Nothing, false))
    );
}

#[test]
fn a_change_to_a_message_of_another_chat_is_refused() {
    assert_eq!(
        plan(&kyoto(), &ChatChange::Branch { message_id: 99 }, false),
        Err(Refused::MessageNotFound(99))
    );
}

#[test]
fn only_a_chat_that_ends_in_a_question_is_answered() {
    assert_eq!(answerable(&unanswered()), Ok(()));
    assert_eq!(answerable(&kyoto()), Err(Refused::NothingToAnswer));
    assert_eq!(answerable(&[]), Err(Refused::NothingToAnswer));
}
