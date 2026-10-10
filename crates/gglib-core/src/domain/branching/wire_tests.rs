//! The bodies a change and a chat's branch points travel as.

use serde_json::json;

use super::fixture::image;
use super::{ChatChange, ChatThread};

#[test]
fn a_change_is_named_by_its_kind() {
    let change: ChatChange =
        serde_json::from_value(json!({"kind": "regenerate", "message_id": 4})).unwrap();
    assert_eq!(change, ChatChange::Regenerate { message_id: 4 });
    let edit = ChatChange::Edit {
        message_id: 3,
        content: "Make it shorter".to_owned(),
        images: Vec::new(),
    };
    assert_eq!(
        serde_json::to_value(&edit).unwrap(),
        json!({"kind": "edit", "message_id": 3, "content": "Make it shorter"})
    );
}

#[test]
fn a_change_with_a_key_it_does_not_take_is_refused() {
    let refused = serde_json::from_value::<ChatChange>(
        json!({"kind": "branch", "message_id": 4, "replace_from": 2}),
    );
    assert!(refused.is_err());
}

#[test]
fn an_edit_names_its_images_by_id() {
    let edit: ChatChange = serde_json::from_value(json!({
        "kind": "edit", "message_id": 3, "content": "", "images": [image(0)],
    }))
    .unwrap();
    assert!(matches!(edit, ChatChange::Edit { images, .. } if images == [image(0)]));
}

#[test]
fn a_chat_with_no_branch_point_and_nothing_to_answer_says_neither() {
    let thread = ChatThread {
        messages: Vec::new(),
        points: Vec::new(),
        answerable: false,
    };
    assert_eq!(
        serde_json::to_value(&thread).unwrap(),
        json!({"messages": []})
    );
}
