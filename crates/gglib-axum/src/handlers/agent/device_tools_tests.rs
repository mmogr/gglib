//! What a paired device's run is offered: the floor, then the opened
//! servers' tools narrowed by a Mac-kept chat's own list, and nothing while
//! the tunnel opens no server.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::ConversationSettings;
use serde_json::json;

use super::{Asked, offered};
use crate::handlers::agent::run_fixture::{settled, state};

const DRAW: &str = "builtin:generate_image";

fn named(tools: &[&str]) -> ConversationSettings {
    ConversationSettings {
        tools: tools.iter().map(|tool| (*tool).to_owned()).collect(),
        ..ConversationSettings::default()
    }
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// With no server opened a device's run is offered the image tool with
/// Draw and nothing else, whatever its chat names.
#[test]
fn with_nothing_opened_only_draw_draws() {
    let chat = named(&["fs:read_file", "search"]);
    assert_eq!(offered(&[], &Asked::of_chat(&chat, true)), [DRAW]);
    assert_eq!(
        offered(&[], &Asked::of_chat(&chat, false)),
        Vec::<String>::new()
    );
    assert_eq!(offered(&[], &Asked::of_phone(false)), Vec::<String>::new());
}

/// A chat that names no tool, and a phone's, is offered every tool of the
/// opened servers, by its qualified name, after the floor.
#[test]
fn every_tool_of_an_opened_server_is_offered_qualified() {
    let opened = strings(&["3:search", "3:extract", "4:crawl"]);
    assert_eq!(offered(&opened, &Asked::of_phone(false)), opened);
    let chat = ConversationSettings::default();
    assert_eq!(
        offered(&opened, &Asked::of_chat(&chat, true)),
        [DRAW, "3:search", "3:extract", "4:crawl"]
    );
}

/// A chat's list keeps the opened tools it names, exactly or by the bare
/// name; a bare form that holds a `:` is not matched by an entry.
#[test]
fn a_chat_naming_tools_narrows_by_exact_or_bare_name() {
    let opened = strings(&["3:search", "3:extract", "4:crawl", "9:3:search"]);
    let chat = named(&["search", "4:crawl"]);
    assert_eq!(
        offered(&opened, &Asked::of_chat(&chat, false)),
        ["3:search", "4:crawl"]
    );
    let qualified = named(&["3:search"]);
    assert_eq!(
        offered(&opened, &Asked::of_chat(&qualified, false)),
        ["3:search"]
    );
}

/// A chat's list only narrows: a tool it names that no opened server has,
/// a builtin or another server's, is never added.
#[test]
fn a_chat_naming_tools_no_open_server_has_gets_none_of_them() {
    let opened = strings(&["3:search"]);
    let chat = named(&["builtin:read_file", "7:shell", "read_file"]);
    assert_eq!(
        offered(&opened, &Asked::of_chat(&chat, false)),
        Vec::<String>::new()
    );
    assert_eq!(offered(&opened, &Asked::of_chat(&chat, true)), [DRAW]);
}

/// A chat with its tools off is offered no MCP tool, and Draw still draws.
#[test]
fn tools_off_offers_no_mcp_tool_but_draw_still_draws() {
    let opened = strings(&["3:search"]);
    let off = ConversationSettings {
        no_tools: Some(true),
        ..ConversationSettings::default()
    };
    assert_eq!(
        offered(&opened, &Asked::of_chat(&off, false)),
        Vec::<String>::new()
    );
    assert_eq!(offered(&opened, &Asked::of_chat(&off, true)), [DRAW]);
}

/// An MCP server for `sh -c` that answers `initialize` and `tools/list`
/// and offers one tool, `echo`.
#[cfg(unix)]
pub(super) const STAND_IN_SERVER: &str = r#"while IFS= read -r line; do
  id=${line#*\"id\":}
  id=${id%%,*}
  case "$line" in
    *'"method":"initialize"'*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{\"tools\":{}},\"serverInfo\":{\"name\":\"stand-in\"}}}" ;;
    *'"method":"tools/list"'*)
      printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"tools\":[{\"name\":\"echo\",\"description\":\"says it back\"}]}}" ;;
  esac
done"#;

/// A server configured and running here is not offered to a device while
/// the live session opens no server, to a phone's run or a chat that names
/// its tool.
#[cfg(unix)]
#[tokio::test]
async fn resolve_offers_the_floor_while_the_gateway_opens_nothing() {
    use gglib_core::ports::RemoteGatewayPort as _;

    use super::resolve;

    let (_dir, state) = state().await;
    let server = gglib_core::NewMcpServer::new_stdio(
        "tavily",
        "sh",
        vec!["-c".to_owned(), STAND_IN_SERVER.to_owned()],
        None,
    );
    let server = state.mcp.add_server(server).await.unwrap();
    let tools = state.mcp.start_server(server.id).await.unwrap();
    assert_eq!(tools.len(), 1, "the stand-in runs");
    assert!(state.remote.gateway().mcp_servers().is_empty());

    let echo = format!("{}:echo", server.id);
    assert_eq!(
        resolve(&state, &Asked::of_phone(false)).await,
        Vec::<String>::new()
    );
    let chat = named(&[echo.as_str(), "echo"]);
    assert_eq!(resolve(&state, &Asked::of_chat(&chat, true)).await, [DRAW]);
    state.mcp.shutdown().await;
}

/// A phone's history may hold a call to an MCP tool and its answer from a
/// server this machine no longer opens, or never ran: it is read into a
/// run by its shape alone, and the run is made.
#[tokio::test]
async fn a_history_holding_another_servers_answered_call_is_read_without_refusal() {
    use crate::handlers::agent::chat_turn::{read, start};

    let (_dir, state) = state().await;
    crate::handlers::agent::image_gate::image_gate_tests::model(&state, "qwen-vision", true).await;
    let body = json!({
        "model": "qwen-vision",
        "messages": [
            { "role": "user", "content": "what is new?" },
            { "role": "assistant", "content": null, "tool_calls": [
                { "id": "c1", "type": "function", "function": {
                    "name": "3:search", "arguments": "{\"query\":\"news\"}" } } ] },
            { "role": "tool", "tool_call_id": "c1", "content": "Nothing much." },
            { "role": "assistant", "content": "Nothing much." },
            { "role": "user", "content": "and now?" },
        ],
    });

    let (_, chat) = read(&state, &body, false.into()).await.unwrap();
    assert!(matches!(
        &chat.messages[1],
        AgentMessage::Assistant { content } if content.tool_calls[0].name == "3:search"
    ));
    assert!(
        matches!(&chat.messages[2], AgentMessage::Tool { tool_call_id, .. } if tool_call_id == "c1")
    );
    let created = start(&state, "phone", "p1", body, false.into())
        .await
        .unwrap();
    assert!(created.created);
    // Nothing serves this harness's model; the run ends there.
    settled(&state).await;
}
