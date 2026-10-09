//! What a resumed chat reprints, and how it shows a turn's images.

use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};

use super::*;

/// A stored message of `role`, with `images` as the history repository
/// returns them: facts, no bytes.
fn stored_message(role: MessageRole, content: &str, images: &[(u32, u32)]) -> Message {
    let images = images.iter().map(|&(width, height)| AttachmentInfo {
        id: AttachmentId::of(&width.to_be_bytes()),
        mime: "image/png".to_owned(),
        width,
        height,
    });
    Message {
        id: 1,
        conversation_id: 1,
        origin_id: None,
        role,
        content: content.to_owned(),
        created_at: String::new(),
        metadata: None,
        images: images.collect(),
    }
}

#[test]
fn a_resumed_chat_shows_a_marker_for_each_image_of_its_last_user_turn() {
    use MessageRole::{Assistant, User};
    let history = [
        stored_message(User, "what is the error?", &[(2560, 1440), (64, 32)]),
        stored_message(Assistant, "A missing semicolon.", &[]),
    ];

    let jogger = memory_jogger(&history, "Agent session");

    let (wide, small) = (&history[0].images[0].id, &history[0].images[1].id);
    assert!(jogger.contains(&format!(
        "You: what is the error? [image 2560x1440 {}] [image 64x32 {}]",
        &wide.as_str()[..8],
        &small.as_str()[..8]
    )));
    assert!(jogger.contains("Assistant: A missing semicolon."));
    assert!(!jogger.contains("semicolon. [image"));
    assert!(!jogger.contains("Tool images"));
}

#[test]
fn a_resumed_chat_with_no_images_shows_no_marker() {
    use MessageRole::User;

    let jogger = memory_jogger(&[stored_message(User, "hello", &[])], "Agent session");

    assert!(jogger.contains("You: hello"));
    assert!(!jogger.contains("[image"));
}

#[test]
fn the_jogger_clips_a_long_turn_on_a_character_and_keeps_its_markers() {
    use MessageRole::User;
    let long = "é".repeat(201);

    let jogger = memory_jogger(&[stored_message(User, &long, &[(64, 32)])], "t");

    assert!(jogger.contains(&format!("You: {}… [image 64x32 ", "é".repeat(200))));
}

#[test]
fn a_resumed_chat_shows_the_images_its_last_turns_tools_made() {
    use MessageRole::{Assistant, Tool, User};
    let history = [
        stored_message(User, "draw a dot", &[]),
        stored_message(Tool, "[image 16x16 PNG stored]", &[(16, 16)]),
        stored_message(User, "and a square", &[]),
        stored_message(Tool, "[image 1024x1024 PNG stored]", &[(1024, 1024)]),
        stored_message(Tool, "no image", &[]),
        stored_message(Assistant, "Here is a square.", &[]),
    ];

    let jogger = memory_jogger(&history, "Agent session");

    let square = &history[3].images[0].id;
    let line = format!("Tool images: [image 1024x1024 {}]", &square.as_str()[..8]);
    assert!(jogger.contains(&line), "{jogger}");
    assert!(
        !jogger.contains("16x16"),
        "an earlier turn's images: {jogger}"
    );
}
