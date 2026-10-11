//! Which drawn picture a model is shown, and on which message.

use serde_json::json;

use super::*;
use crate::domain::agent::{AssistantContent, ToolCall};

fn id(of: &str) -> AttachmentId {
    AttachmentId::of(of.as_bytes())
}

fn call(id: &str, tool: &str) -> AgentMessage {
    AgentMessage::Assistant {
        content: AssistantContent {
            text: None,
            tool_calls: vec![ToolCall {
                id: id.to_owned(),
                name: tool.to_owned(),
                arguments: json!({ "prompt": "a fox" }),
            }],
        },
    }
}

fn made(call_id: &str, images: &[&str]) -> AgentMessage {
    AgentMessage::Tool {
        tool_call_id: call_id.to_owned(),
        content: "Drew 1 image, 1024x1024 PNG, with m in 9 s; the user can see it, you cannot."
            .to_owned(),
        images: images.iter().map(|image| id(image)).collect(),
    }
}

fn said(text: &str) -> AgentMessage {
    AgentMessage::Assistant {
        content: AssistantContent {
            text: Some(text.to_owned()),
            tool_calls: Vec::new(),
        },
    }
}

/// A chat in which one picture was drawn, up to its closing reply.
fn one_drawn() -> Vec<AgentMessage> {
    vec![
        AgentMessage::System {
            content: "be brief".to_owned(),
        },
        AgentMessage::user("draw a fox"),
        call("c1", DRAW_TOOL_NAME),
        made("c1", &["fox"]),
        said("I asked for a fox."),
    ]
}

#[test]
fn the_message_after_a_drawn_picture_carries_it() {
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("what is in the corner?"));

    let fox = id("fox");
    assert_eq!(
        shown_picture(&chat, true),
        Some(ShownPicture { at: 5, image: &fox })
    );
}

#[test]
fn a_model_that_reads_no_images_is_shown_none() {
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("what is in the corner?"));

    assert_eq!(shown_picture(&chat, false), None);
}

#[test]
fn the_picture_stays_on_the_same_message_as_the_chat_goes_on() {
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("what is in the corner?"));
    let first = shown_picture(&chat, true).map(|shown| (shown.at, shown.image.clone()));

    chat.push(said("A tree."));
    chat.push(AgentMessage::user("and the sky?"));
    chat.push(said("Blue."));
    chat.push(AgentMessage::user("thanks"));
    let later = shown_picture(&chat, true).map(|shown| (shown.at, shown.image.clone()));

    assert_eq!(first, Some((5, id("fox"))));
    assert_eq!(later, first);
}

#[test]
fn only_the_newer_of_two_pictures_is_shown_on_the_message_after_it() {
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("draw it bluer"));
    chat.push(call("c2", DRAW_TOOL_NAME));
    chat.push(made("c2", &["blue fox"]));
    chat.push(said("I asked for a blue fox."));
    chat.push(AgentMessage::user("which is better?"));

    let blue = id("blue fox");
    assert_eq!(
        shown_picture(&chat, true),
        Some(ShownPicture {
            at: 9,
            image: &blue
        })
    );
}

#[test]
fn the_older_picture_is_shown_until_the_newer_one_is_made() {
    // The first request of the second run sent with Draw: the model writes
    // the new prompt while looking at the picture it is asked to change.
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("draw it bluer"));

    let fox = id("fox");
    assert_eq!(
        shown_picture(&chat, true),
        Some(ShownPicture { at: 5, image: &fox })
    );
}

#[test]
fn a_picture_no_message_follows_is_not_shown() {
    assert_eq!(shown_picture(&one_drawn(), true), None);

    // The newer picture has no message after it, and the older one is not
    // shown in its place.
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("draw it bluer"));
    chat.push(call("c2", DRAW_TOOL_NAME));
    chat.push(made("c2", &["blue fox"]));
    assert_eq!(shown_picture(&chat, true), None);
}

#[test]
fn the_closing_request_of_a_run_sent_with_draw_is_shown_nothing() {
    for told in [DRAW_THEN_SAY, DRAW_THEN_SAY_FAILED] {
        let mut chat = one_drawn();
        chat.pop();
        chat.push(AgentMessage::user(told));

        assert_eq!(shown_picture(&chat, true), None, "{told}");
    }
}

#[test]
fn another_tools_image_is_never_shown() {
    for tool in ["3:generate_image", "generate_image", "builtin:screenshot"] {
        let chat = vec![
            AgentMessage::user("look"),
            call("c1", tool),
            made("c1", &["shot"]),
            AgentMessage::user("what is it?"),
        ];

        assert_eq!(shown_picture(&chat, true), None, "{tool}");
    }
}

#[test]
fn another_tools_later_image_does_not_hide_the_drawn_picture() {
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("now look at the page"));
    chat.push(call("c2", "3:screenshot"));
    chat.push(made("c2", &["shot"]));
    chat.push(AgentMessage::user("what is it?"));

    let fox = id("fox");
    assert_eq!(
        shown_picture(&chat, true),
        Some(ShownPicture { at: 5, image: &fox })
    );
}

#[test]
fn a_call_id_used_again_by_another_tool_is_not_the_drawing_tools() {
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("now look at the page"));
    chat.push(call("c1", "3:screenshot"));
    chat.push(made("c1", &["shot"]));
    chat.push(AgentMessage::user("what is it?"));

    // The fox, from the first use of `c1`; never the screenshot.
    let fox = id("fox");
    assert_eq!(shown_picture(&chat, true).map(|s| s.image), Some(&fox));
}

#[test]
fn a_draw_that_made_no_picture_shows_none_and_hides_none() {
    let mut chat = one_drawn();
    chat.push(AgentMessage::user("draw it bluer"));
    chat.push(call("c2", DRAW_TOOL_NAME));
    chat.push(AgentMessage::tool("c2", "the image model is busy"));
    chat.push(said("It failed."));
    chat.push(AgentMessage::user("oh well"));

    let fox = id("fox");
    assert_eq!(
        shown_picture(&chat, true),
        Some(ShownPicture { at: 5, image: &fox })
    );
}

#[test]
fn of_several_images_one_call_made_the_last_is_shown() {
    let chat = vec![
        AgentMessage::user("draw two"),
        call("c1", DRAW_TOOL_NAME),
        made("c1", &["one", "two"]),
        AgentMessage::user("which?"),
    ];

    let two = id("two");
    assert_eq!(shown_picture(&chat, true).map(|s| s.image), Some(&two));
}

#[test]
fn the_line_takes_back_the_tools_you_cannot_and_asks_for_nothing() {
    assert!(DRAWN_PICTURE_LINE.contains("You can see it now"));
    assert!(DRAWN_PICTURE_LINE.contains("not written by the person"));
    assert!(!DRAWN_PICTURE_LINE.contains('?'));
}
