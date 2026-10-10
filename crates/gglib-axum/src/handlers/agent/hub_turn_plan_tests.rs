//! A device's turn read against the hub's record: the history it sends,
//! the tools it may call and the model it runs on.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewMessage};
use gglib_core::ports::RemoteGatewayPort as _;

use super::hub_turn_tests::{chat, turn};
use super::{plan, tools_of};
use crate::handlers::agent::hub_model::model_for;
use crate::handlers::agent::run_fixture::state;

#[tokio::test]
async fn the_history_is_the_prompt_the_rows_and_the_new_message() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let plan = plan(&state, turn(id, "second")).await.unwrap();
    let user = |c: &str| AgentMessage::User {
        content: c.to_owned(),
        images: Vec::new(),
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

/// A device's turn calls no tool while the tunnel's owner keeps the tunnel
/// closed to this machine's MCP tools, which it is until `enable
/// --allow-mcp`: not even the tools the chat names.
#[tokio::test]
async fn a_device_turn_calls_no_tool_while_the_tunnel_is_closed_to_them() {
    let (_dir, state) = state().await;
    let named = ConversationSettings {
        max_iterations: Some(4),
        tools: vec!["fs:read_file".to_owned()],
        ..ConversationSettings::default()
    };
    let named = chat(&state, Some(named)).await;
    assert!(!state.remote.gateway().mcp_allowed(), "closed by default");
    let plan = plan(&state, turn(named, "second")).await.unwrap();
    assert_eq!(plan.chat.tool_filter, Some(Vec::new()));
    assert_eq!(plan.chat.config.and_then(|c| c.max_iterations), Some(4));
}

/// With the tunnel open to them, only the tools the chat names: none when
/// it names none or turned them off, never every tool.
#[test]
fn with_the_tunnel_open_a_device_turn_may_call_only_the_tools_the_chat_names() {
    let named = ConversationSettings {
        tools: vec!["fs:read_file".to_owned()],
        ..ConversationSettings::default()
    };
    let off = ConversationSettings {
        no_tools: Some(true),
        ..named.clone()
    };
    let read_file = vec!["fs:read_file".to_owned()];
    assert_eq!(tools_of(&named, true, false), read_file);
    assert_eq!(tools_of(&named, false, false), Vec::<String>::new());
    assert_eq!(
        tools_of(&ConversationSettings::default(), true, false),
        Vec::<String>::new()
    );
    assert_eq!(tools_of(&off, true, false), Vec::<String>::new());
}

/// Draw adds exactly the image tool's qualified name, whatever the tunnel's
/// switch and the chat's `no_tools` say, and nothing without it: every cell
/// of tunnel open or closed, tools on or off, drawn or not.
#[test]
fn draw_adds_exactly_the_qualified_image_tool_in_every_case() {
    let named = ConversationSettings {
        tools: vec!["fs:read_file".to_owned()],
        ..ConversationSettings::default()
    };
    let off = ConversationSettings {
        no_tools: Some(true),
        ..named.clone()
    };
    let draw = "builtin:generate_image".to_owned();
    let read_file = "fs:read_file".to_owned();
    for (settings, mcp_allowed, mcp) in [
        (&named, true, vec![read_file]),
        (&named, false, Vec::new()),
        (&off, true, Vec::new()),
        (&off, false, Vec::new()),
    ] {
        assert_eq!(tools_of(settings, mcp_allowed, false), mcp);
        let mut drawn = mcp.clone();
        drawn.push(draw.clone());
        assert_eq!(tools_of(settings, mcp_allowed, true), drawn);
    }
}

/// An MCP server's tool that happens to be called `generate_image` is not
/// reachable by a device's draw turn while the tunnel is closed to MCP
/// tools: the filter such a turn runs under lists the builtin and refuses
/// the other, listed or called.
#[tokio::test]
async fn a_draw_turn_never_reaches_an_mcp_tool_named_generate_image() {
    use std::sync::Arc;

    use gglib_core::domain::agent::{ToolCall, ToolDefinition, ToolResult};
    use gglib_core::ports::{FilteredToolExecutor, ToolExecutorPort};

    /// Lists and runs the builtin and an MCP server's namesake.
    struct Both;

    #[async_trait::async_trait]
    impl ToolExecutorPort for Both {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![
                ToolDefinition::new("builtin:generate_image"),
                ToolDefinition::new("3:generate_image"),
                ToolDefinition::new("3:builtin:generate_image"),
                ToolDefinition::new("3:read_file"),
            ]
        }

        async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::text(
                call.id.clone(),
                format!("ran {}", call.name),
                true,
            ))
        }
    }

    let named = ConversationSettings {
        tools: vec!["read_file".to_owned()],
        ..ConversationSettings::default()
    };
    let filter = tools_of(&named, false, true).into_iter().collect();
    let tools = FilteredToolExecutor::new(Arc::new(Both), filter);

    let listed: Vec<String> = tools
        .list_tools()
        .await
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(listed, ["builtin:generate_image"]);
    let call = |name: &str| ToolCall {
        id: "c1".to_owned(),
        name: name.to_owned(),
        arguments: serde_json::json!({}),
    };
    // Nor is a tool whose own name is the builtin's qualified one, colon
    // included: the filter's entry names the builtin and nothing else.
    for theirs in ["3:generate_image", "3:builtin:generate_image"] {
        let theirs = tools.execute(&call(theirs)).await;
        assert!(
            theirs.map_or(true, |result| !result.success
                && !result.content.contains("ran ")),
            "the MCP server's tool ran for a device without --allow-mcp"
        );
    }
    let ours = tools
        .execute(&call("builtin:generate_image"))
        .await
        .unwrap();
    assert_eq!(ours.content, "ran builtin:generate_image");
}

/// A turn sent with Draw pressed is planned with the image tool and nothing
/// else, and its model is offered exactly that; one sent without is offered
/// none.
#[tokio::test]
async fn a_draw_turn_offers_its_model_the_image_tool_and_no_other() {
    use gglib_core::domain::hub_chats::HubTurn;

    use crate::handlers::agent::run_fixture::drawing_state;
    use crate::handlers::agent::turn_fixture::{model, sent};

    let (_dir, state) = drawing_state().await;
    let served = model(&state, |_| {}).await;
    let id = chat(&state, None).await;
    let pressed = HubTurn {
        draw: true,
        ..turn(id, "a fox in the snow")
    };

    let planned = plan(&state, pressed).await.unwrap();

    assert!(planned.chat.draw);
    assert_eq!(
        planned.chat.tool_filter,
        Some(vec!["builtin:generate_image".to_owned()])
    );
    let offered = |body: &serde_json::Value| -> Vec<String> {
        let tools = body["tools"].as_array().cloned().unwrap_or_default();
        tools
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap().to_owned())
            .collect()
    };
    let body = sent(&state, served, planned.chat).await;
    assert_eq!(offered(&body), ["builtin:generate_image"]);

    let plain = plan(&state, turn(id, "hello")).await.unwrap();
    assert!(!plain.chat.draw);
    assert_eq!(plain.chat.tool_filter, Some(Vec::new()));
    let body = sent(&state, served, plain.chat).await;
    assert_eq!(offered(&body), Vec::<String>::new());
}

/// A turn sent with Draw pressed on a hub that cannot draw is refused,
/// `drawing_unavailable`, with nothing written and no run made.
#[tokio::test]
async fn a_draw_turn_the_hub_cannot_draw_for_is_refused_and_writes_nothing() {
    use gglib_core::domain::hub_chats::HubTurn;
    use gglib_core::ports::RunsPort as _;

    use super::hub_turn_tests::{device, refused};
    use crate::handlers::agent::run_fixture::saved;

    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let pressed = HubTurn {
        draw: true,
        ..turn(id, "a fox in the snow")
    };

    let refusal = refused(super::start(&state, "phone", "d1", pressed).await);

    assert_eq!(refusal, (400, "drawing_unavailable"));
    assert_eq!(saved(&state, id).await.len(), 2);
    assert!(state.runs.list(&device("phone")).runs.is_empty());
    assert_eq!(state.agent_semaphore.available_permits(), 1);
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
            images: Vec::new(),
        })
        .await
        .unwrap();
    let plan = plan(&state, turn(id, "second")).await.unwrap();
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
        model_id.to_string(),
        "the catalogued model, by its id"
    );
}
