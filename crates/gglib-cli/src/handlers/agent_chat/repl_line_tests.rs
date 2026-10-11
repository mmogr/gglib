//! The lines of a chat: which message carries which image.

use gglib_core::domain::attachment::AttachmentId;

use super::*;
use crate::handlers::agent_chat::images::images_tests::{attached, file, png, service};

/// The message `input` sends: its text and the ids of its images.
async fn sent(input: &str, images: &mut TurnImages<'_>) -> (String, Vec<AttachmentId>) {
    match read(input, images).await {
        Line::Send(AgentMessage::User { content, images }) => (content, images),
        other => panic!("{input:?} sends a user message, not {other:?}"),
    }
}

/// `gglib chat --image a.png --image b.png`: the first message typed
/// carries both, in the order given, and the one after it carries none.
#[tokio::test]
async fn the_first_message_carries_the_images_the_flag_named_and_the_next_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (first, second) = (png(64, 32), png(32, 64));
    let paths = [file(&dir, "a.png", &first), file(&dir, "b.png", &second)];
    let (service, _) = service();
    let (mut images, _) = attached(&service, &paths, true).await.unwrap();

    let (text, carried) = sent("what are these?", &mut images).await;

    assert_eq!(text, "what are these?");
    assert_eq!(
        carried,
        [AttachmentId::of(&first), AttachmentId::of(&second)]
    );
    assert_eq!(sent("and now?", &mut images).await.1, []);
}

/// `/image <path>` sends nothing: it answers the receipt, and the next
/// message typed carries the image.
#[tokio::test]
async fn an_image_line_attaches_the_file_to_the_next_message_typed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = png(64, 32);
    let path = file(&dir, "shot.png", &bytes);
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[], true).await.unwrap();

    let line = read(&format!("/image {}", path.display()), &mut images).await;

    let Line::Image(receipt) = line else {
        panic!("an /image line is no message: {line:?}");
    };
    assert_eq!(receipt, "  image shot.png: 64x32, ~2 tokens");
    let (text, carried) = sent("what is this?", &mut images).await;
    assert_eq!(text, "what is this?");
    assert_eq!(carried, [AttachmentId::of(&bytes)]);
    assert_eq!(sent("and now?", &mut images).await.1, []);
}

/// The other commands and an empty line send nothing, and an image that
/// waits for a message goes on waiting through them.
#[tokio::test]
async fn a_command_sends_nothing_and_leaves_a_waiting_image_for_the_message() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = png(64, 32);
    let path = file(&dir, "shot.png", &bytes);
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[path], true).await.unwrap();

    assert!(matches!(read("", &mut images).await, Line::Empty));
    assert!(matches!(read("/help", &mut images).await, Line::Help));
    assert!(matches!(read("/image", &mut images).await, Line::Image(_)));
    assert!(matches!(read("/quit", &mut images).await, Line::Quit));
    assert!(matches!(read("/exit", &mut images).await, Line::Quit));

    let (_, carried) = sent("what is this?", &mut images).await;
    assert_eq!(carried, [AttachmentId::of(&bytes)]);
}

/// `/retry`, `/edit`, `/branch` and `/branches` ask for a change to the
/// chat, or its branches, and send nothing; a word that only starts as one
/// of them is a message.
#[tokio::test]
async fn a_change_to_the_chat_is_asked_for_and_not_sent() {
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[], true).await.unwrap();

    for (typed, ask) in [
        ("/retry", Ask::Retry),
        ("/branch", Ask::Branch),
        (
            "/edit Make it shorter",
            Ask::Edit("Make it shorter".to_owned()),
        ),
        ("/edit\t  two  words ", Ask::Edit("two  words".to_owned())),
        ("/edit", Ask::Edit(String::new())),
    ] {
        match read(typed, &mut images).await {
            Line::Change(asked) => assert_eq!(asked, ask, "{typed}"),
            other => panic!("{typed} read as {other:?}"),
        }
    }
    assert!(matches!(
        read("/branches", &mut images).await,
        Line::Branches
    ));
    assert!(matches!(
        read("/editor of mine", &mut images).await,
        Line::Send(_)
    ));
}

/// `/draw` is the Draw switch, never a message.
#[tokio::test]
async fn a_draw_line_is_the_switch_and_sends_nothing() {
    let (service, _) = service();
    let (mut images, _) = attached(&service, &[], true).await.unwrap();
    assert!(matches!(read("/draw", &mut images).await, Line::Draw));
    assert!(matches!(
        read("/draw a fox", &mut images).await,
        Line::Send(_)
    ));
}
