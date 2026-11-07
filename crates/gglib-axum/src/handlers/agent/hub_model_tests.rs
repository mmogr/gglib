//! The model a device's turn runs on: the one its chat stored, by id and
//! never on another machine; when it names none, the one running on the
//! hub, before the hub's default.

use gglib_app_services::types::ServerInfo;
use gglib_core::domain::chat::{
    Conversation, ConversationSettings, Message, MessageRole, NewConversation,
};
use gglib_core::domain::{Machine, ModelRef};

use super::{choose, serving};
use crate::error::HttpError;
use crate::handlers::agent::run_fixture::state;
use crate::state::AppState;

fn server(model_id: i64, model_name: &str, port: u16, started_at: u64) -> ServerInfo {
    ServerInfo {
        model_id,
        model_name: model_name.to_owned(),
        pid: None,
        port,
        started_at,
        runtime: gglib_core::domain::RuntimeKind::Llama,
        launch: None,
    }
}

async fn registered(state: &AppState, name: &str) -> i64 {
    registered_at(state, name, &format!("/models/{name}.gguf")).await
}

/// A catalogue entry called `name`, from the file at `path`: two files may
/// carry one name.
async fn registered_at(state: &AppState, name: &str, path: &str) -> i64 {
    let model = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(path),
        7.0,
        chrono::Utc::now(),
    );
    state.core.models().add(model).await.unwrap().id
}

/// A chat that names no model anywhere.
async fn bare(state: &AppState) -> (Conversation, Vec<Message>) {
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    (
        history.get_conversation(id).await.unwrap().unwrap(),
        Vec::new(),
    )
}

#[tokio::test]
async fn a_chat_naming_no_model_runs_on_the_one_running() {
    let (_dir, state) = state().await;
    let (conversation, rows) = bare(&state).await;
    let running = [server(7, "running-7b", 9000, 10)];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "7", "by its id");
}

/// Of several, the one started last; of those started together, the lowest
/// port.
#[tokio::test]
async fn of_several_running_the_one_started_last() {
    let (_dir, state) = state().await;
    let (conversation, rows) = bare(&state).await;
    // Neither first nor last, so taking either end of the list is wrong.
    let running = [
        server(3, "late-high", 9002, 20),
        server(2, "late-low", 9001, 20),
        server(1, "early", 9000, 10),
    ];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "2", "late-low");
}

#[tokio::test]
async fn nothing_running_and_no_default_says_to_start_a_model() {
    let (_dir, state) = state().await;
    let (conversation, rows) = bare(&state).await;
    match choose(&state, &conversation, &rows, &[]).await {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => {
            assert_eq!((status.as_u16(), code), (422, "no_model"));
            assert_eq!(
                message,
                "this chat has no model and nothing is running on the hub: start a model there"
            );
        }
        other => panic!("not refused as no_model: {:?}", other.map(|_| ())),
    }
}

/// The chat's own model, its settings' and its last reply's each come
/// before the one running.
#[tokio::test]
async fn what_the_chat_names_wins_over_what_is_running() {
    let (_dir, state) = state().await;
    let running = [server(9, "running-7b", 9000, 10)];
    let (mut conversation, _) = bare(&state).await;
    let reply = Message {
        id: 1,
        conversation_id: conversation.id,
        origin_id: None,
        role: MessageRole::Assistant,
        content: "answer".to_owned(),
        created_at: String::new(),
        metadata: Some(serde_json::json!({ "modelName": "replied-model" })),
        images: Vec::new(),
    };
    let rows = [reply];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "replied-model", "its last reply's");

    conversation.settings = Some(ConversationSettings {
        model_name: Some("set-model".to_owned()),
        ..ConversationSettings::default()
    });
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "set-model", "its settings'");

    let own = registered(&state, "own-model").await;
    conversation.model_id = Some(own);
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), own.to_string(), "its own, by id");
}

/// The hub's default only when nothing runs.
#[tokio::test]
async fn the_one_running_comes_before_the_hubs_default() {
    let (_dir, state) = state().await;
    let mut settings = state.core.settings().get().await.unwrap();
    let default = registered(&state, "default-model").await;
    settings.default_model_id = Some(default);
    state.core.settings().save(&settings).await.unwrap();
    let (conversation, rows) = bare(&state).await;
    let running = [server(9, "running-7b", 9000, 10)];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "9");
    let model = choose(&state, &conversation, &rows, &[]).await;
    assert_eq!(model.unwrap(), default.to_string());
}

/// A chat that stored its model as this machine's is run by that id, before
/// anything else it names, and a running server is found by that id: a
/// second model of the same name, running or not, is not it.
#[tokio::test]
async fn a_chat_that_stored_its_model_here_runs_on_that_id_whatever_its_name() {
    let (_dir, state) = state().await;
    let first = registered_at(&state, "twin", "/models/a/twin.gguf").await;
    let second = registered_at(&state, "twin", "/models/b/twin.gguf").await;
    let (mut conversation, rows) = bare(&state).await;
    conversation.settings = Some(ConversationSettings {
        model_name: Some("twin".to_owned()),
        model: Some(ModelRef {
            machine: Machine::Local,
            id: second,
        }),
        ..ConversationSettings::default()
    });
    let running = [
        server(first, "twin", 9000, 10),
        server(second, "twin", 9001, 5),
    ];

    let model = choose(&state, &conversation, &rows, &running)
        .await
        .unwrap();

    assert_eq!(model, second.to_string(), "not the first by name");
    assert_eq!(serving(&running, second), Some(9001), "its own server");
    assert_eq!(serving(&running, 404), None);
}

/// A chat that stored the paired machine's model `qwen3`, its id there 3,
/// on a hub that pairs with `desk` and has a model of that name itself.
async fn chosen_on_desk(state: &AppState) -> Conversation {
    let mut settings = state.core.settings().get().await.unwrap();
    settings.remote_pairing = Some(gglib_core::RemotePairing {
        ticket: "not-a-ticket".to_owned(),
        api_key: "key".to_owned(),
        default_model: None,
        port: None,
        name: Some("desk".to_owned()),
    });
    state.core.settings().save(&settings).await.unwrap();
    registered(state, "qwen3").await;
    let (mut conversation, _) = bare(state).await;
    conversation.settings = Some(ConversationSettings {
        model_name: Some("qwen3".to_owned()),
        model: Some(ModelRef {
            machine: Machine::Paired {
                fingerprint: "0123456789ab".to_owned(),
            },
            id: 3,
        }),
        ..ConversationSettings::default()
    });
    conversation
}

/// The user's first message on chat `conversation_id`, saved.
fn first_message(conversation_id: i64) -> Message {
    Message {
        id: 1,
        conversation_id,
        origin_id: None,
        role: MessageRole::User,
        content: "hi".to_owned(),
        created_at: String::new(),
        metadata: None,
        images: Vec::new(),
    }
}

/// A chat that ran on the paired machine is refused, 409, and its model is
/// never looked up here as a name, even when a model of that name is here.
/// The device asking is told neither that machine's name nor its
/// fingerprint.
#[tokio::test]
async fn a_chat_that_ran_on_the_paired_machine_is_refused() {
    let (_dir, state) = state().await;
    let conversation = chosen_on_desk(&state).await;
    let rows = [first_message(conversation.id)];
    let running = [server(1, "qwen3", 9000, 10)];

    match choose(&state, &conversation, &rows, &running).await {
        Err(HttpError::Conflict(message)) => {
            assert_eq!(
                message,
                "this chat ran on another machine, and its model is that machine's, not this \
                 one's"
            );
            assert!(!message.contains("0123"), "no fingerprint: {message}");
            assert!(!message.contains("desk"), "no host name: {message}");
        }
        other => panic!("not refused as a conflict: {other:?}"),
    }
}

/// A chat naming no model runs on the chat server started last, never on an
/// sd-server started after it: a model that draws cannot chat.
#[tokio::test]
async fn a_chat_naming_no_model_passes_over_an_image_server() {
    let (_dir, state) = state().await;
    let (conversation, rows) = bare(&state).await;
    let mut draws = server(9, "flux", 9001, 20);
    draws.runtime = gglib_core::domain::RuntimeKind::StableDiffusion;
    let running = [server(7, "running-7b", 9000, 10), draws];

    let model = choose(&state, &conversation, &rows, &running).await;

    assert_eq!(
        model.unwrap(),
        "7",
        "the chat server, not the later sd-server"
    );
}

/// A chat that chose the paired machine's model and has no saved message has
/// run nowhere yet: a device's turn on it runs on the model running here,
/// and else on the hub's default, as a chat that names none does. Never on
/// the model here that has the name it stored, which is that machine's.
#[tokio::test]
async fn a_chat_that_only_chose_the_paired_machines_model_runs_here() {
    let (_dir, state) = state().await;
    let conversation = chosen_on_desk(&state).await;
    let running = [server(9, "running-7b", 9000, 10)];

    let model = choose(&state, &conversation, &[], &running).await;
    assert_eq!(model.unwrap(), "9", "the one running, not `qwen3`");

    let mut settings = state.core.settings().get().await.unwrap();
    let default = registered(&state, "default-model").await;
    settings.default_model_id = Some(default);
    state.core.settings().save(&settings).await.unwrap();
    let model = choose(&state, &conversation, &[], &[]).await;
    assert_eq!(model.unwrap(), default.to_string(), "the hub's default");
}

/// With nothing running and no default, it is refused as a chat that names
/// no model is, and not as one that ran on the paired machine.
#[tokio::test]
async fn a_chat_that_only_chose_the_paired_machines_model_and_nothing_runs_has_no_model() {
    let (_dir, state) = state().await;
    let conversation = chosen_on_desk(&state).await;
    match choose(&state, &conversation, &[], &[]).await {
        Err(HttpError::Coded { status, code, .. }) => {
            assert_eq!((status.as_u16(), code), (422, "no_model"));
        }
        other => panic!("not refused as no_model: {:?}", other.map(|_| ())),
    }
}
