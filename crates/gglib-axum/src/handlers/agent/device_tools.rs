//! The tools a turn's run is offered: a paired device's, and this
//! machine's own on one of its chats.
//!
//! Both of a device's doors come here: a turn on one of this machine's
//! chats (`hub_turn`) and a chat the phone keeps itself (`chat_turn`). A
//! device reaches no MCP tool but those of the servers open to paired
//! devices (`gglib mcp allow-remote <server>`, a flag on each server), each
//! by its qualified name `{id}:{tool}`, narrowed by a chat's own tool list
//! when this machine keeps the chat, and never gglib's own file tools, which
//! a device's run has no folder for. With Draw, the image tool beside them.
//! The servers are read when the run starts, and only while a tunnel session
//! is live, and a lazy one is started then, in the run's own work and never
//! in the `PUT` that made it. Which of them the run may use is its
//! [`DeviceServers`]: all of them for a Mac-kept chat, and for a phone-kept
//! one only those whose ids its query names. Those ids are filtered against the servers
//! open to devices in `McpService::opened_tools`, and only there, before any
//! server starts; [`offered`] trusts the list it is handed. The decisions
//! are ADR 0024, ADR 0025 and ADR 0026.
//!
//! This machine's own turn on one of its chats is planned without reading a
//! server ([`Asked::of_local`]): it is offered every tool here, of every
//! enabled server and the builtins, narrowed by the chat's tool list, and
//! none when the chat turned its tools off or its model cannot call any. Its
//! late step starts the enabled servers it reads before its tools are listed
//! ([`settle`]).
//! gglib's file tools join them when the turn has a folder, and the image
//! tool with Draw, both as the loop is composed (`compose::with_folder`,
//! `compose::with_drawing`).

use std::collections::BTreeSet;

use gglib_core::DeviceServers;
use gglib_core::domain::chat::ConversationSettings;
use gglib_core::ports::{RemoteGatewayPort as _, names_whole_server};
use gglib_mcp::BUILTIN_PREFIX;

use super::AgentChatRequest;
use super::compose::DRAW_TOOL;
use crate::state::AppState;

/// What a turn's run asks of this machine's tools, before any server is
/// read.
#[derive(Debug, Clone, Default)]
pub(super) struct Asked {
    /// The chat's own tool list: empty for all, else the tools it names
    /// exactly or by their bare name.
    narrow: Vec<String>,
    /// Whose run it is, and what that lets it reach.
    reach: Reach,
    /// The message was sent with Draw pressed: the image tool.
    draw: bool,
}

/// What a run may reach of this machine's tools.
#[derive(Debug, Clone)]
enum Reach {
    /// A paired device's run: which of the servers open to devices it may
    /// use, none when the chat turned its tools off, and none unless
    /// something says otherwise.
    Opened(DeviceServers),
    /// This machine's own run: every tool here when `calls`, and otherwise
    /// none.
    Here { calls: bool },
}

/// Which enabled servers this machine's own run starts before its tools are
/// read.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Starts {
    Every,
    Only(BTreeSet<i64>),
}

impl Default for Reach {
    fn default() -> Self {
        Self::Opened(DeviceServers::default())
    }
}

impl Asked {
    /// A turn on a chat this machine keeps, under that chat's settings.
    pub(super) fn of_chat(settings: &ConversationSettings, draw: bool) -> Self {
        Self {
            narrow: settings.tools.clone(),
            reach: Reach::Opened(if settings.no_tools == Some(true) {
                DeviceServers::none()
            } else {
                DeviceServers::All
            }),
            draw,
        }
    }

    /// This machine's own turn on one of its chats, under that chat's
    /// settings, on a model that `calls_tools` or does not, as the
    /// catalogue says (`hub_model::calls_tools`).
    pub(super) fn of_local(settings: &ConversationSettings, draw: bool, calls_tools: bool) -> Self {
        Self {
            narrow: settings.tools.clone(),
            reach: Reach::Here {
                calls: calls_tools && settings.no_tools != Some(true),
            },
            draw,
        }
    }

    /// A chat the phone keeps itself, which names no tool of its own, only
    /// the ids of the open servers it may use (`servers`, from its query's
    /// `tools`): never every open server, which only a Mac-kept chat is.
    pub(super) fn of_phone(draw: bool, servers: BTreeSet<i64>) -> Self {
        Self {
            reach: Reach::Opened(DeviceServers::Only(servers)),
            draw,
            ..Self::default()
        }
    }

    /// What the run is offered whatever servers are open: the image tool
    /// with Draw, by its qualified name and never its bare one, else
    /// nothing. Draw overrides the chat's tools being off, and its servers
    /// whichever they are: the button is the person's choice for that
    /// message.
    pub(super) fn floor(&self) -> Vec<String> {
        self.draw
            .then(|| DRAW_TOOL.to_owned())
            .into_iter()
            .collect()
    }

    /// The tool filter the run is planned with. A device's run: the
    /// [`floor`](Self::floor), until the servers open to devices are read
    /// once it starts ([`settle`]). This machine's own: final already, with
    /// no server read: every tool (`None`) when the chat names none, the
    /// tools it names, or none at all (`[]`) when its tools are off or its
    /// model calls none. Never the image tool, which the loop adds with
    /// Draw after the folder's file tools, so a `[]` that Draw names it in
    /// gains no file tool (`compose::prepare_over`).
    pub(super) fn planned(&self) -> Option<Vec<String>> {
        match self.reach {
            Reach::Opened(_) => Some(self.floor()),
            Reach::Here { calls: false } => Some(Vec::new()),
            Reach::Here { calls: true } => (!self.narrow.is_empty()).then(|| self.narrow.clone()),
        }
    }

    /// The enabled servers this machine's own run reads tools from, which
    /// its late step starts first ([`settle`]): none (`None`) for a device's
    /// run, or one that calls no tool; every one when its chat names no
    /// tool, or names one by its bare name, which any server may have; else
    /// those whose ids its list names (`{id}:{tool}`, or `{id}:*` for every
    /// tool of one).
    fn here_servers(&self) -> Option<Starts> {
        if !matches!(self.reach, Reach::Here { calls: true }) {
            return None;
        }
        let mut ids = BTreeSet::new();
        for entry in &self.narrow {
            if entry.starts_with(BUILTIN_PREFIX) {
                continue;
            }
            let id = entry.split_once(':').and_then(|(id, _)| id.parse().ok());
            let Some(id) = id else {
                return Some(Starts::Every);
            };
            ids.insert(id);
        }
        if self.narrow.is_empty() {
            return Some(Starts::Every);
        }
        Some(Starts::Only(ids))
    }

    /// The servers open to devices that a device's run may use; none for
    /// this machine's own run, which is offered no server that way.
    fn opened(&self) -> Option<&DeviceServers> {
        match &self.reach {
            Reach::Opened(servers) if !servers.is_none() => Some(servers),
            _ => None,
        }
    }

    /// Whether the chat's own list names `tool`, an opened tool's qualified
    /// name: exactly, by its bare name when the entry has no `:`, or as one
    /// of every tool of its server (`{id}:*`, [`names_whole_server`]). Any
    /// other entry with a `:` is some tool's whole qualified name.
    fn names(&self, tool: &str) -> bool {
        let bare = tool.split_once(':').map_or(tool, |(_, bare)| bare);
        self.narrow.iter().any(|entry| {
            entry == tool
                || (!entry.contains(':') && entry == bare)
                || names_whole_server(entry, tool)
        })
    }
}

/// The tools a run that `asked` is offered, of the `opened` servers' tools
/// (qualified), already only those of the servers it may use: the floor,
/// then every opened tool the chat's list names, all of them when it names
/// none, none when it may use no server. Never a tool that is not opened,
/// whatever the chat names.
pub(super) fn offered(opened: &[String], asked: &Asked) -> Vec<String> {
    let mut tools = asked.floor();
    if asked.opened().is_some() {
        tools.extend(
            opened
                .iter()
                .filter(|tool| asked.narrow.is_empty() || asked.names(tool))
                .cloned(),
        );
    }
    tools
}

/// The tools a device's run that `asked` is offered on this machine now:
/// the floor, unless a tunnel session is live and the run may use some
/// server, when the servers open to devices that it may use are started as
/// needed and their tools read. This machine's own run reads no server
/// here: its filter is [`Asked::planned`].
pub(super) async fn resolve(state: &AppState, asked: &Asked) -> Vec<String> {
    let Some(servers) = asked.opened() else {
        return asked.floor();
    };
    if !state.remote.gateway().serving() {
        return asked.floor();
    }
    offered(&state.mcp.opened_tools(servers).await, asked)
}

/// `chat` with its tool filter set to what a run that `asked` is offered on
/// this machine now: a device's run settles its tools this way once its
/// model is ready ([`resolve`]). This machine's own run keeps the filter it
/// was planned with ([`Asked::planned`]), once the enabled servers it reads
/// are started (`McpService::start_enabled`).
pub(super) async fn settle(
    state: &AppState,
    mut chat: AgentChatRequest,
    asked: &Asked,
) -> AgentChatRequest {
    if matches!(asked.reach, Reach::Here { .. }) {
        match asked.here_servers() {
            Some(Starts::Every) => state.mcp.start_enabled(None).await,
            Some(Starts::Only(ids)) => state.mcp.start_enabled(Some(&ids)).await,
            None => {}
        }
        return chat;
    }
    chat.tool_filter = Some(resolve(state, asked).await);
    chat
}

#[cfg(test)]
#[path = "device_tools_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "device_tools_wired_tests.rs"]
mod wired_tests;
