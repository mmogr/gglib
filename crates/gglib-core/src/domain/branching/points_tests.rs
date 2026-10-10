//! The options a family holds at each turn of one of its chats.

use super::fixture::{chat, family, row};
use super::{BranchPoint, points};
use crate::domain::chat::MessageRole::{Assistant, User};

fn shown(point: &BranchPoint) -> Vec<(i64, Option<i64>)> {
    point
        .options
        .iter()
        .map(|option| (option.conversation_id, option.message_id))
        .collect()
}

#[test]
fn a_chat_alone_has_no_branch_point() {
    let alone = vec![chat(
        10,
        1,
        vec![row(1, 1, User, "Hi"), row(2, 2, Assistant, "Hello.")],
    )];
    assert!(points(10, &alone).is_empty());
}

/// Chat 10's third turn has three options: its own question, the edited
/// one of chat 20, and none of chat 30's, which copies chat 10's.
#[test]
fn the_options_are_the_different_turns_after_what_the_chats_share() {
    let family = family();
    let points = points(10, &family);
    assert_eq!(points.len(), 2, "{points:?}");
    let asked = &points[0];
    assert_eq!(asked.message_id, Some(3));
    assert_eq!(shown(asked), [(10, Some(3)), (20, Some(13))]);
    assert_eq!(asked.index, 0);
    let answered = &points[1];
    assert_eq!(answered.message_id, Some(4));
    assert_eq!(shown(answered), [(10, Some(4)), (30, Some(24))]);
}

#[test]
fn each_chat_of_the_family_sees_the_same_options_from_its_side() {
    let family = family();
    let from_the_edit = points(20, &family);
    assert_eq!(shown(&from_the_edit[0]), [(30, Some(23)), (20, Some(13))]);
    assert_eq!(from_the_edit[0].index, 1);
    let from_the_regenerate = points(30, &family);
    assert_eq!(
        shown(&from_the_regenerate[0]),
        [(30, Some(23)), (20, Some(13))]
    );
    assert_eq!(
        shown(&from_the_regenerate[1]),
        [(10, Some(4)), (30, Some(24))]
    );
}

/// Chats 10 and 30 hold the same question; chat 20 is shown it by the one
/// that changed last.
#[test]
fn an_option_two_chats_hold_is_shown_by_the_one_changed_last() {
    let family = family();
    let points = points(20, &family);
    assert_eq!(points[0].options[0].conversation_id, 30);
}

/// Chat 40 was branched after the first reply and holds nothing since.
#[test]
fn a_branch_with_nothing_yet_is_an_empty_option_after_the_others() {
    let family = family();
    let points = points(40, &family);
    assert_eq!(points.len(), 1, "{points:?}");
    let next = &points[0];
    assert_eq!(next.message_id, None);
    assert_eq!(shown(next), [(30, Some(23)), (20, Some(13)), (40, None)]);
    assert_eq!(next.index, 2);
    assert_eq!(next.options[2].preview, "");
}

#[test]
fn an_option_is_shown_by_its_turn_s_line() {
    let family = family();
    let points = points(10, &family);
    let previews: Vec<&str> = points[1]
        .options
        .iter()
        .map(|o| o.preview.as_str())
        .collect();
    assert_eq!(previews, ["Hostels and buses.", "Hostels, buses, bentos."]);
}

/// Only a turn is an option: two chats that part inside a reply have no
/// branch point there.
#[test]
fn a_difference_inside_a_reply_is_no_branch_point() {
    let family = vec![
        chat(
            10,
            1,
            vec![
                row(1, 1, User, "Q"),
                row(2, 2, Assistant, ""),
                row(3, 3, Assistant, "A"),
            ],
        ),
        chat(
            20,
            2,
            vec![
                row(11, 1, User, "Q"),
                row(12, 2, Assistant, ""),
                row(13, 13, Assistant, "B"),
            ],
        ),
    ];
    assert!(points(10, &family).is_empty(), "{:?}", points(10, &family));
}
