//! The tools a paired device's run is offered.
//!
//! Both of a device's doors come here: a turn on one of this machine's
//! chats (`hub_turn`) and a chat the phone keeps itself (`chat_turn`). A
//! device reaches no MCP tool but those of the servers `gglib remote enable
//! --allow-mcp <server>` names, each by its qualified name `{id}:{tool}`,
//! narrowed by a chat's own tool list when this machine keeps the chat, and
//! never gglib's own file tools, which a device's run has no folder for. With
//! Draw, the image tool beside them. The servers are named by the live
//! session's gateway, read when the run starts, and a lazy one is started
//! then, in the run's own work and never in the `PUT` that made it. The
//! decision is ADR 0024.

use gglib_core::domain::chat::ConversationSettings;
use gglib_core::ports::RemoteGatewayPort as _;

use super::compose::DRAW_TOOL;
use crate::state::AppState;

/// What a device's run asks of this machine's tools, before the servers
/// the tunnel opens are read.
#[derive(Debug, Clone, Default)]
pub(super) struct Asked {
    /// The chat's own tool list: empty for all, else the opened tools it
    /// names exactly or by their bare name.
    narrow: Vec<String>,
    /// The chat turned its tools off: no MCP tool.
    no_tools: bool,
    /// The message was sent with Draw pressed: the image tool.
    draw: bool,
}

impl Asked {
    /// A turn on a chat this machine keeps, under that chat's settings.
    pub(super) fn of_chat(settings: &ConversationSettings, draw: bool) -> Self {
        Self {
            narrow: settings.tools.clone(),
            no_tools: settings.no_tools == Some(true),
            draw,
        }
    }

    /// A chat the phone keeps itself, which names no tool of its own and
    /// turns them all off with `no_tools` (`tools=none`).
    pub(super) fn of_phone(draw: bool, no_tools: bool) -> Self {
        Self {
            no_tools,
            draw,
            ..Self::default()
        }
    }

    /// What the run is offered whatever the tunnel opens: the image tool
    /// with Draw, by its qualified name and never its bare one, else
    /// nothing. Draw overrides the chat's `no_tools`: the button is the
    /// person's choice for that message.
    pub(super) fn floor(&self) -> Vec<String> {
        self.draw
            .then(|| DRAW_TOOL.to_owned())
            .into_iter()
            .collect()
    }

    /// Whether the chat's own list names `tool`, an opened tool's qualified
    /// name: exactly, or by its bare name when the entry has no `:`. An
    /// entry with one is some tool's whole qualified name.
    fn names(&self, tool: &str) -> bool {
        let bare = tool.split_once(':').map_or(tool, |(_, bare)| bare);
        self.narrow
            .iter()
            .any(|entry| entry == tool || (!entry.contains(':') && entry == bare))
    }
}

/// The tools a run that `asked` is offered, of the `opened` servers' tools
/// (qualified): the floor, then every opened tool the chat's list names,
/// all of them when it names none, none when its tools are off. Never a
/// tool that is not opened, whatever the chat names.
pub(super) fn offered(opened: &[String], asked: &Asked) -> Vec<String> {
    let mut tools = asked.floor();
    if !asked.no_tools {
        tools.extend(
            opened
                .iter()
                .filter(|tool| asked.narrow.is_empty() || asked.names(tool))
                .cloned(),
        );
    }
    tools
}

/// The tools a run that `asked` is offered on this machine now: the
/// floor, unless the live session opens servers and the chat's tools are
/// on, when the named servers are started as needed and their tools read.
pub(super) async fn resolve(state: &AppState, asked: &Asked) -> Vec<String> {
    let names = state.remote.gateway().mcp_servers();
    if names.is_empty() || asked.no_tools {
        return asked.floor();
    }
    offered(&state.mcp.opened_tools(&names).await, asked)
}

#[cfg(test)]
#[path = "device_tools_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "device_tools_wired_tests.rs"]
mod wired_tests;
