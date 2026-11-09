//! A turn on a model of this machine, once it is loaded, is refused when a
//! message carries an image its model cannot read, and a run on the paired
//! machine's model is not judged here. A turn refused before its model is
//! loaded is in `hub_turn_images_tests.rs`.

use std::path::PathBuf;

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::{AttachmentId, Machine, ModelRef, NewModel};

use super::super::chat_sampling::ChatSampling;
use super::super::hub_model::Loaded;
use super::super::loop_request::LoopRequest;
use super::super::remote_upstream::{Upstream, endpoint, far};
use super::super::run_fixture::state;
use crate::error::HttpError;
use crate::state::AppState;

/// A PNG's signature and `IHDR`, 640 by 480: all the store reads of one.
fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(640_u32.to_be_bytes());
    bytes.extend(480_u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

/// An uploaded image's id.
pub(in crate::handlers::agent) async fn uploaded(state: &AppState) -> AttachmentId {
    let stored = state.core.attachments().ingest(&png()).await.unwrap();
    stored.info.id
}

/// A catalogue model named `name`, linked to a projector or not.
pub(in crate::handlers::agent) async fn model(state: &AppState, name: &str, sees: bool) -> i64 {
    let new = NewModel::new(
        name.to_owned(),
        PathBuf::from("models").join(format!("{name}.gguf")),
        7.0,
        chrono::Utc::now(),
    );
    let mut model = state.core.models().add(new).await.expect("added");
    if sees {
        model.projector_path = Some(PathBuf::from("models").join("mmproj-F16.gguf"));
        state.core.models().update(&model).await.expect("linked");
    }
    model.id
}

/// The upstream of a turn whose messages are `asked`, once model `id` is
/// loaded, through an endpoint that is never dialled.
async fn loaded(state: &AppState, asked: &LoopRequest, id: i64) -> Result<Upstream, HttpError> {
    let door = gglib_runtime::Door {
        root_url: "http://127.0.0.1:9950".to_owned(),
        key: Some("k".to_owned()),
        name: None,
    };
    endpoint(state, asked, Loaded { id, port: 9951 }, door).await
}

/// A turn whose messages are `messages`.
fn request(messages: Vec<AgentMessage>) -> LoopRequest {
    LoopRequest {
        messages,
        ..LoopRequest::default()
    }
}

fn with_image(text: &str, image: &AttachmentId) -> AgentMessage {
    AgentMessage::User {
        content: text.to_owned(),
        images: vec![image.clone()],
    }
}

/// A turn on a model with no projector is refused by name when its last
/// message carries an image: the model's name and the command that links
/// one.
#[tokio::test]
async fn a_local_run_with_an_image_is_refused_for_a_model_with_no_projector() {
    let (_dir, state) = state().await;
    let image = uploaded(&state).await;
    let blind = model(&state, "blind-7b", false).await;
    let asked = request(vec![with_image("what is this?", &image)]);

    let Err(HttpError::Coded {
        status,
        code,
        message,
    }) = loaded(&state, &asked, blind).await
    else {
        panic!("the run was not refused by code");
    };

    assert_eq!((status.as_u16(), code), (400, "model_cannot_read_images"));
    assert!(message.contains("'blind-7b'"), "{message}");
    assert!(
        message.contains("gglib model update blind-7b --projector"),
        "{message}"
    );
}

/// The whole history is sent each turn, so an image in an earlier message
/// is refused as one in the last is.
#[tokio::test]
async fn a_local_run_counts_the_images_of_its_history() {
    let (_dir, state) = state().await;
    let image = uploaded(&state).await;
    let blind = model(&state, "blind-7b", false).await;
    let asked = request(vec![
        with_image("what is this?", &image),
        AgentMessage::user("and in words?"),
    ]);

    let refusal = loaded(&state, &asked, blind).await.err();

    assert!(matches!(
        refusal,
        Some(HttpError::Coded {
            code: "model_cannot_read_images",
            ..
        })
    ));
}

/// A model with a projector takes the image, a model with none takes text,
/// and a model the catalogue no longer holds is not judged.
#[tokio::test]
async fn a_local_run_goes_on_when_the_model_can_see_or_nothing_is_to_be_seen() {
    let (_dir, state) = state().await;
    let image = uploaded(&state).await;
    let sees = model(&state, "sees-7b", true).await;
    let blind = model(&state, "blind-7b", false).await;
    let pictured = request(vec![with_image("what is this?", &image)]);
    let text = request(vec![AgentMessage::user("hello")]);

    assert!(loaded(&state, &pictured, sees).await.is_ok());
    assert!(loaded(&state, &text, blind).await.is_ok());
    assert!(loaded(&state, &pictured, 424_242).await.is_ok());
}

/// A model of the paired machine is not judged here, although this
/// machine's catalogue holds no such model: the turn goes to the far
/// branch, which with nothing connected says so.
#[tokio::test]
async fn a_far_run_with_an_image_is_not_refused_here() {
    let (_dir, state) = state().await;
    model(&state, "blind-7b", false).await;
    let named = ModelRef {
        machine: Machine::Paired {
            fingerprint: "0a1b2c3d4e5f".to_owned(),
        },
        id: 1,
    };

    let reach = async |machine: &Machine| state.remote.far_for(machine).await;
    let Err(err) = far(reach, &named, &ChatSampling::default()).await else {
        panic!("nothing is connected");
    };

    assert!(
        matches!(&err, HttpError::Conflict(m) if m.contains("not connected")),
        "got {err:?}"
    );
}
