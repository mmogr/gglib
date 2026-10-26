//! The image model a person chose, at every door that runs a message sent
//! with Draw: the page's run, a device's turn on a hub chat, and a chat run
//! with builtins. It goes with `draw` or is refused; one that cannot draw
//! refuses the request with its own code before anything is written; and it
//! reaches the drawing tool's request and nothing the chat model is sent.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::contracts::http::images::DrawingAvailability;
use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::{
    ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest, RunScope,
    RunsPort as _,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::AgentChatRequest;
use super::chat_turn::{self, ChatQuery, Drawing};
use super::dto::AgentRunRequest;
use super::run::create_run;
use super::run_fixture::{saved, state};
use super::turn_fixture::{DRAWS, model, turn};
use crate::error::HttpError;
use crate::state::AppState;

/// An id no image model here has.
const GONE: u32 = 404;
/// A chat model's id.
const CHATS: u32 = 5;
/// An image model with a file missing.
const LACKS: u32 = 6;
/// An image model that can draw, and its name.
const PICKED: u32 = 4242;
const PICKED_NAME: &str = "qwen-image-PICKED";

/// What each id that cannot draw is refused with.
const REFUSALS: [(u32, &str, &str); 3] = [
    (GONE, "model_not_found", "the image model chosen is gone"),
    (CHATS, "not_an_image_model", "that model chats"),
    (LACKS, "image_model_incomplete", "that model lacks its vae"),
];

/// An image driver that judges a chosen id as [`REFUSALS`] says, draws with
/// `flux1-schnell` when none is chosen, and keeps every request it is
/// handed, answering each with a failure.
#[derive(Debug, Default)]
struct Judges {
    asked: Mutex<Vec<ImageRequest>>,
}

#[async_trait]
impl ImageGenerationPort for Judges {
    async fn generate(
        &self,
        request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        self.asked.lock().unwrap().push(request);
        Err(ImageError::Failed {
            message: "a test's driver draws nothing".to_owned(),
        })
    }

    async fn drawing(&self, chosen: Option<u32>) -> DrawingAvailability {
        let refusal = REFUSALS.iter().find(|(id, ..)| Some(*id) == chosen);
        match (refusal, chosen) {
            (Some((_, code, why)), _) => DrawingAvailability::refused(*code, *why),
            (None, Some(id)) => DrawingAvailability {
                model_id: Some(id),
                ..DrawingAvailability::with(PICKED_NAME)
            },
            (None, None) => DrawingAvailability::with("flux1-schnell"),
        }
    }
}

/// A daemon whose image driver is a [`Judges`], and that driver.
async fn judging() -> (tempfile::TempDir, AppState, Arc<Judges>) {
    let (dir, state) = state().await;
    let mut state = Arc::into_inner(state).expect("the only handle");
    let driver = Arc::new(Judges::default());
    state.images = Arc::clone(&driver) as Arc<dyn ImageGenerationPort>;
    (dir, Arc::new(state), driver)
}

/// A hub chat on `model_id`, with no rows.
async fn hub_chat(state: &AppState, model_id: i64) -> i64 {
    let settings = ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Local,
            id: model_id,
        }),
        ..ConversationSettings::default()
    };
    state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(settings),
        })
        .await
        .unwrap()
}

/// A device's turn on `chat` that says `draw` and `image_model` as given.
fn hub_turn(chat: i64, draw: bool, image_model: Option<u32>) -> HubTurn {
    HubTurn {
        conversation_id: chat,
        content: "a fox in the snow".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw,
        image_model,
    }
}

/// The page's run body on `chat`, with `more`.
fn page_run(chat: i64, more: &Value) -> Value {
    let mut body = json!({
        "port": 0,
        "messages": [{ "role": "user", "content": "a fox in the snow" }],
        "conversation_id": chat,
    });
    let extra = more.as_object().unwrap().clone();
    body.as_object_mut().unwrap().extend(extra);
    body
}

/// A phone's own chat request on the catalogue model `catalogue-name`.
fn phone_request() -> Value {
    json!({
        "model": "catalogue-name",
        "messages": [{ "role": "user", "content": "a fox in the snow" }],
    })
}

/// A refusal's status, code and words.
fn refused<T>(result: Result<T, HttpError>) -> (u16, &'static str, String) {
    match result {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => (status.as_u16(), code, message),
        Err(other) => panic!("uncoded: {other}"),
        Ok(_) => panic!("not refused"),
    }
}

/// Each door's answer to a request that says `draw` and `image_model` as
/// given, each on a chat of its own, with what would show that the door
/// wrote something: the page's chat and the hub's chat.
async fn at_every_door(
    state: &AppState,
    served: i64,
    draw: bool,
    image_model: Option<u32>,
) -> ([(&'static str, (u16, &'static str, String)); 4], [i64; 2]) {
    let mut said = serde_json::Map::new();
    if draw {
        said.insert("draw".to_owned(), json!(true));
    }
    if let Some(id) = image_model {
        said.insert("image_model".to_owned(), json!(id));
    }
    let said = Value::Object(said);
    let page_chat = hub_chat(state, served).await;
    let hub = hub_chat(state, served).await;

    let page = refused(create_run(state, "r1", page_run(page_chat, &said)).await);
    let device = refused(
        super::hub_turn::start(state, "phone", "d1", hub_turn(hub, draw, image_model)).await,
    );
    // Drawn with no server named: `tools=none&draw=true`.
    let query = ChatQuery {
        drawing: Drawing { draw, image_model },
        ..ChatQuery::default()
    };
    let kept = refused(chat_turn::start(state, "phone", "p1", phone_request(), query).await);
    let mut route = serde_json::from_value::<AgentRunRequest>(page_run(page_chat, &said))
        .unwrap()
        .chat;
    route.port = 1;
    let route = match super::chat(axum::extract::State(state.clone()), axum::Json(route)).await {
        Ok(_) => panic!("the chat route ran"),
        Err(error) => refused::<()>(Err(error)),
    };
    (
        [
            ("the page's run", page),
            ("a device's turn", device),
            ("a chat run with builtins", kept),
            ("the chat route", route),
        ],
        [page_chat, hub],
    )
}

/// Nothing was written, no run made and no slot kept, and the driver was
/// never asked to draw.
async fn assert_nothing_happened(state: &AppState, driver: &Judges, chats: [i64; 2]) {
    for chat in chats {
        assert!(saved(state, chat).await.is_empty(), "no row was written");
    }
    assert!(state.runs.list(&RunScope::Local).runs.is_empty());
    let phone = RunScope::Device("phone".to_owned());
    assert!(state.runs.list(&phone).runs.is_empty(), "no run was made");
    assert_eq!(state.agent_semaphore.available_permits(), 1);
    assert!(driver.asked.lock().unwrap().is_empty(), "nothing was drawn");
}

/// `image_model` without `draw` is refused `invalid_request` at every door,
/// even for a model that could draw: the run would be offered no image
/// tool, and the choice dropped in silence.
#[tokio::test]
async fn an_image_model_without_draw_is_refused_at_every_door() {
    let (_dir, state, driver) = judging().await;
    let served = model(&state, |_| {}).await;

    let (answers, chats) = at_every_door(&state, served, false, Some(PICKED)).await;

    for (door, (status, code, why)) in answers {
        assert_eq!((status, code), (400, "invalid_request"), "{door}");
        assert!(
            why.contains("`image_model` goes with `draw`"),
            "{door}: {why}"
        );
    }
    assert_nothing_happened(&state, &driver, chats).await;
}

/// A chosen model that is gone, is not an image model or lacks a file
/// refuses the request with that code and its sentence at every door,
/// before a row is written, a run made or a slot taken. It is not replaced
/// by the default, which here could draw.
#[tokio::test]
async fn a_chosen_model_that_cannot_draw_is_refused_with_its_code_and_writes_nothing() {
    for (id, code, why) in REFUSALS {
        let (_dir, state, driver) = judging().await;
        let served = model(&state, |_| {}).await;

        let (answers, chats) = at_every_door(&state, served, true, Some(id)).await;

        for (door, answer) in answers {
            assert_eq!(answer, (400, code, why.to_owned()), "{door}");
        }
        assert_nothing_happened(&state, &driver, chats).await;
    }
}

/// The chat request each door reads a message sent with Draw and
/// `image_model` as: the page's run, a device's turn, a chat run with
/// builtins.
async fn read_at_each_door(
    state: &AppState,
    served: i64,
    image_model: Option<u32>,
) -> [(&'static str, AgentChatRequest); 3] {
    let mut said = json!({ "draw": true });
    if let Some(id) = image_model {
        said["image_model"] = json!(id);
    }
    let page: AgentRunRequest =
        serde_json::from_value(page_run(hub_chat(state, served).await, &said)).unwrap();
    let page = super::run::plan(state, page).await.unwrap().0;
    let hub = hub_turn(hub_chat(state, served).await, true, image_model);
    let device = super::hub_turn::plan(state, hub).await.unwrap().chat;
    let drawing = Drawing {
        draw: true,
        image_model,
    };
    let (_, kept) = chat_turn::read(state, &phone_request(), drawing)
        .await
        .unwrap();
    [
        ("the page's run", page),
        ("a device's turn", device),
        ("a chat run with builtins", kept),
    ]
}

/// From every door the chosen id is the model of the request the drawing
/// tool hands the driver, and is in nothing sent to the chat model: not the
/// tool it is offered, not an argument, not a message. Nor is the image
/// model's name, or the key.
#[tokio::test]
async fn the_chosen_model_reaches_the_drawing_request_and_nothing_the_chat_model_is_sent() {
    let (_dir, state, driver) = judging().await;
    let served = model(&state, |_| {}).await;

    for (door, chat) in read_at_each_door(&state, served, Some(PICKED)).await {
        assert_eq!(chat.image_model, Some(PICKED), "{door}");
        driver.asked.lock().unwrap().clear();

        let turn = turn(&state, served, chat, DRAWS).await;

        let asked = driver.asked.lock().unwrap().clone();
        assert_eq!(asked.len(), 1, "{door}: drawn once");
        assert_eq!(asked[0].model, Some(PICKED.to_string()), "{door}");
        assert_eq!(turn.requests.len(), 2, "{door}");
        for request in &turn.requests {
            let sent = request.to_string();
            for secret in [PICKED.to_string().as_str(), PICKED_NAME, "image_model"] {
                assert!(!sent.contains(secret), "{door}: {secret} in {sent}");
            }
        }
        let tool = &turn.requests[0]["tools"][0]["function"];
        assert_eq!(tool["name"], "builtin:generate_image", "{door}");
        let mut arguments: Vec<&String> = tool["parameters"]["properties"]
            .as_object()
            .expect("the tool's arguments")
            .keys()
            .collect();
        arguments.sort();
        assert_eq!(arguments, ["n", "prompt", "seed", "size"], "{door}");
    }
}

/// A message sent with Draw and no choice hands the driver a request that
/// names no model, from every door: the default rule draws.
#[tokio::test]
async fn a_draw_with_no_choice_names_no_model() {
    let (_dir, state, driver) = judging().await;
    let served = model(&state, |_| {}).await;

    for (door, chat) in read_at_each_door(&state, served, None).await {
        assert_eq!(chat.image_model, None, "{door}");
        driver.asked.lock().unwrap().clear();
        turn(&state, served, chat, DRAWS).await;
        let asked = driver.asked.lock().unwrap().clone();
        assert_eq!(asked.len(), 1, "{door}");
        assert_eq!(asked[0].model, None, "{door}");
    }
}
