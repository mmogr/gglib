//! A device's turn read against the hub's record: the history it sends,
//! the tools it may call and the model it runs on.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewMessage};
use gglib_core::ports::RemoteGatewayPort as _;

use super::hub_turn_tests::{chat, turn};
use super::plan;
use crate::handlers::agent::hub_model::model_for;
use crate::handlers::agent::run_fixture::state;

#[tokio::test]
async fn the_history_is_the_prompt_the_rows_and_the_new_message() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let plan = plan(&state, turn(id, "second"), false).await.unwrap();
    let user = |c: &str| AgentMessage::User {
        content: c.to_owned(),
    };
    let wire: Vec<String> = plan
        .chat
        .messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap())
        .collect();
    let want: Vec<String> = [
        AgentMessage::System {
            content: "Be brief.".to_owned(),
        },
        user("first"),
        AgentMessage::Assistant {
            content: gglib_core::domain::agent::AssistantContent {
                text: Some("answer".to_owned()),
                tool_calls: Vec::new(),
            },
        },
        user("second"),
    ]
    .iter()
    .map(|m| serde_json::to_string(m).unwrap())
    .collect();
    assert_eq!(wire, want);
    assert_eq!(plan.model, "qwen3-8b", "the model of the last reply");
    assert!(plan.chat.config.is_none());
    assert_eq!(
        plan.chat.tool_filter,
        Some(Vec::new()),
        "no tool, ever by default"
    );
}

/// The tools a device's turn may call: none while the tunnel is closed to
/// this machine's MCP tools, and when it is open only those the chat names.
#[tokio::test]
async fn a_device_turn_calls_no_tool_unless_the_tunnel_may_reach_them() {
    let (_dir, state) = state().await;
    let named = ConversationSettings {
        max_iterations: Some(4),
        tools: vec!["fs:read_file".to_owned()],
        ..ConversationSettings::default()
    };
    let off = ConversationSettings {
        no_tools: Some(true),
        ..named.clone()
    };
    let named = chat(&state, Some(named)).await;
    let off = chat(&state, Some(off)).await;
    let unnamed = chat(&state, None).await;
    let read_file = Some(vec!["fs:read_file".to_owned()]);
    let none = Some(Vec::new());
    for (id, allowed, want) in [
        (named, false, &none),
        (unnamed, false, &none),
        (named, true, &read_file),
        (unnamed, true, &none),
        (off, true, &none),
    ] {
        let plan = plan(&state, turn(id, "second"), allowed).await.unwrap();
        assert_eq!(&plan.chat.tool_filter, want, "chat {id}, allowed {allowed}");
    }
    let plan = plan(&state, turn(named, "second"), false).await.unwrap();
    assert_eq!(plan.chat.config.and_then(|c| c.max_iterations), Some(4));
    assert!(
        !state.remote.gateway().mcp_allowed(),
        "closed until `enable --allow-mcp`"
    );
}

/// The prompt is the conversation's; a saved system row is not sent again.
#[tokio::test]
async fn a_saved_system_row_is_left_out_of_the_history() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    state
        .core
        .chat_history()
        .save_message(NewMessage {
            conversation_id: id,
            role: MessageRole::System,
            content: "OLD-PROMPT".to_owned(),
            metadata: None,
        })
        .await
        .unwrap();
    let plan = plan(&state, turn(id, "second"), false).await.unwrap();
    let systems: Vec<&AgentMessage> = plan
        .chat
        .messages
        .iter()
        .filter(|m| matches!(m, AgentMessage::System { .. }))
        .collect();
    assert_eq!(systems.len(), 1);
    assert!(matches!(systems[0], AgentMessage::System { content } if content == "Be brief."));
}

#[tokio::test]
async fn the_chats_own_model_comes_before_the_one_it_last_used() {
    let (_dir, state) = state().await;
    let model = gglib_core::domain::NewModel::new(
        "catalogued".to_owned(),
        std::path::PathBuf::from("/models/c.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    let model_id = state.core.models().add(model).await.unwrap().id;
    let id = chat(&state, None).await;
    let history = state.core.chat_history();
    let mut conversation = history.get_conversation(id).await.unwrap().unwrap();
    let rows = history.get_messages(id).await.unwrap();
    assert_eq!(
        model_for(&state, &conversation, &rows).await.unwrap(),
        "qwen3-8b"
    );
    conversation.model_id = Some(model_id);
    assert_eq!(
        model_for(&state, &conversation, &rows).await.unwrap(),
        "catalogued"
    );
}
