//! A device's turn that carries an image: refused before the chat's model
//! is loaded when that model cannot read one, when the image was never
//! uploaded, or when its images and the history's are over 16 MiB together,
//! and a turn all the same when it is its image alone.

use gglib_core::domain::AttachmentId;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{MessageRole, NewMessage};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::ports::RunsPort as _;
use gglib_core::request_pipeline::MAX_IMAGE_BYTES;

use super::hub_turn_tests::{chat, device, failed_with, refused};
use super::{plan, start};
use crate::handlers::agent::image_gate::image_gate_tests::{model, uploaded};
use crate::handlers::agent::run_fixture::{saved, state};

/// The model the chat fixture's last reply was made by, so the one its
/// next turn runs on.
const CHAT_MODEL: &str = "qwen3-8b";

fn image_turn(conversation_id: i64, content: &str, image: &AttachmentId) -> HubTurn {
    HubTurn {
        conversation_id,
        content: content.to_owned(),
        images: vec![image.clone()],
        thinking: None,
        answer_saved: false,
        draw: false,
    }
}

/// A device's turn with an image on a chat whose model has no projector is
/// refused before the model is loaded: the refusal is the image's, not the
/// load's, and nothing is written.
#[tokio::test]
async fn a_hub_turn_with_an_image_is_refused_before_the_model_is_loaded() {
    let (_dir, state) = state().await;
    let image = uploaded(&state).await;
    model(&state, CHAT_MODEL, false).await;
    let id = chat(&state, None).await;

    let turn = image_turn(id, "what is this?", &image);
    let refusal = refused(start(&state, "phone", "d1", turn).await);

    assert_eq!(refusal, (400, "model_cannot_read_images"));
    assert_eq!(saved(&state, id).await.len(), 2);
    assert!(state.runs.list(&device("phone")).runs.is_empty());
}

/// The positive control: the same turn on a model that can see gets as far
/// as loading it, which in this fixture is what fails.
#[tokio::test]
async fn a_hub_turn_with_an_image_goes_on_to_load_a_model_that_can_see() {
    let (_dir, state) = state().await;
    let image = uploaded(&state).await;
    model(&state, CHAT_MODEL, true).await;
    let id = chat(&state, None).await;

    let turn = image_turn(id, "what is this?", &image);
    let created = start(&state, "phone", "d1", turn).await.unwrap();

    assert!(created.created, "the turn passed the image's gate");
    assert_eq!(
        failed_with(&state, "phone", "d1").await,
        "model_unavailable"
    );
}

/// A text turn on a chat whose saved rows carry an image is refused too:
/// the hub sends the rows again.
#[tokio::test]
async fn a_hub_turn_counts_the_images_of_the_saved_rows() {
    let (_dir, state) = state().await;
    let image = uploaded(&state).await;
    model(&state, CHAT_MODEL, false).await;
    let id = chat(&state, None).await;
    let row = NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "an earlier screenshot".to_owned(),
        metadata: None,
        images: vec![image],
    };
    state.core.chat_history().save_message(row).await.unwrap();

    let turn = HubTurn {
        conversation_id: id,
        content: "and in words?".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
    };
    let refusal = refused(plan(&state, turn).await);

    assert_eq!(refusal, (400, "model_cannot_read_images"));
}

/// A turn that is its image alone is a turn: the message it adds has no
/// text and the image, by id. One with neither is refused.
#[tokio::test]
async fn a_hub_turn_is_text_or_an_image() {
    let (_dir, state) = state().await;
    let image = uploaded(&state).await;
    model(&state, CHAT_MODEL, true).await;
    let id = chat(&state, None).await;

    for text in ["", "  \n "] {
        let plan = plan(&state, image_turn(id, text, &image)).await.unwrap();
        let last = plan.chat.messages.last().unwrap();
        let AgentMessage::User { content, images } = last else {
            panic!("the turn is the user's");
        };
        assert_eq!(
            (content.as_str(), images.as_slice()),
            (text, &[image.clone()][..])
        );
    }

    let neither = HubTurn {
        conversation_id: id,
        content: " ".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
    };
    assert_eq!(
        refused(plan(&state, neither).await),
        (400, "invalid_request")
    );
}

/// A turn naming an image that was never uploaded is refused by that code
/// before the model is loaded, and nothing is written.
#[tokio::test]
async fn a_hub_turn_naming_an_image_never_uploaded_is_refused_before_the_load() {
    let (_dir, state) = state().await;
    model(&state, CHAT_MODEL, true).await;
    let id = chat(&state, None).await;
    let never = AttachmentId::of(b"never uploaded");

    let turn = image_turn(id, "what is this?", &never);
    let refusal = refused(start(&state, "phone", "d1", turn).await);

    assert_eq!(refusal, (400, "attachment_not_found"));
    assert_eq!(saved(&state, id).await.len(), 2);
    assert!(state.runs.list(&device("phone")).runs.is_empty());
}

/// A stored PNG of `len` bytes, its tail all `fill`.
async fn stored(state: &crate::state::AppState, fill: u8, len: usize) -> AttachmentId {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(640_u32.to_be_bytes());
    bytes.extend(480_u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes.resize(len.max(bytes.len()), fill);
    state
        .core
        .attachments()
        .ingest(&bytes)
        .await
        .unwrap()
        .info
        .id
}

/// The saved rows' images and the turn's add up: an 8 MiB image in the
/// history and the same again in the turn are the 16 MiB a request may
/// carry, and one image more is refused by its code before the model is
/// loaded, with nothing written.
#[tokio::test]
async fn a_hub_turn_whose_images_and_the_historys_are_over_16_mib_is_refused_before_the_load() {
    let (_dir, state) = state().await;
    let big = stored(&state, 2, MAX_IMAGE_BYTES).await;
    let small = stored(&state, 3, 64).await;
    model(&state, CHAT_MODEL, true).await;
    let id = chat(&state, None).await;
    let row = NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "an earlier screenshot".to_owned(),
        metadata: None,
        images: vec![big.clone()],
    };
    state.core.chat_history().save_message(row).await.unwrap();

    let mut over = image_turn(id, "and these?", &big);
    over.images.push(small);
    let refusal = refused(start(&state, "phone", "d1", over).await);

    assert_eq!(refusal, (400, "request_images_too_large"));
    assert_eq!(saved(&state, id).await.len(), 3);
    assert!(state.runs.list(&device("phone")).runs.is_empty());

    let at = image_turn(id, "and this?", &big);
    start(&state, "phone", "d1", at).await.unwrap();
    assert_eq!(
        failed_with(&state, "phone", "d1").await,
        "model_unavailable"
    );
}
