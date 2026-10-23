//! The MCP servers this machine opens to its paired devices, by name.
//!
//! `gglib remote enable --allow-mcp <server>` names them, and the names are
//! checked here, once, when the tunnel is enabled: a name that opens no one
//! server is refused then, so a typo cannot read as a grant. A name is the
//! one thing the owner types and sees, which is why the grant is by name and
//! not by the id the agent loop qualifies a tool with.
//!
//! When a paired device's run starts, [`McpService::opened_tools`] turns the
//! names into the tools that run is offered, by the id each tool is called
//! by, starting a named server that is not running. That is the run's own
//! work, done once its model is ready, not the `PUT`'s that made it.

use gglib_core::{McpServer, McpServerStatus, McpServerType, McpServiceError};
use tracing::warn;

use crate::service::{McpService, RESERVED_SERVER_NAME};
use crate::tool_executor::qualified;

/// The one server `name` names: exactly one has it, and it is not gglib's own.
///
/// Two servers sharing a name name neither: an older database can hold such
/// a pair, and opening both, or whichever was listed first, would open a
/// server the owner did not mean.
fn the_one<'a>(servers: &'a [McpServer], name: &str) -> Option<&'a McpServer> {
    if name == RESERVED_SERVER_NAME {
        return None;
    }
    let mut named = servers.iter().filter(|server| server.name == name);
    match (named.next(), named.next()) {
        (Some(server), None) => Some(server),
        _ => None,
    }
}

/// Why [`McpService::refuse_unopenable`] refused.
#[derive(Debug)]
pub enum Unopenable {
    /// A name that opens no one server, said in words a person can act on:
    /// the caller's to fix.
    Name(String),
    /// The servers could not be read, so no name could be checked: this
    /// machine's failure, not the caller's.
    Unread(McpServiceError),
}

impl McpService {
    /// Refuse `names` unless each names exactly one configured server.
    ///
    /// Naming none reads nothing, so a plain enable is never refused for a
    /// repository it did not need.
    ///
    /// # Errors
    ///
    /// [`Unopenable::Name`] for the first name that is blank, is `builtin`
    /// (gglib's own tools, which a device is never opened to by name), names
    /// no server, or names more than one; [`Unopenable::Unread`] when the
    /// servers could not be read.
    pub async fn refuse_unopenable(&self, names: &[String]) -> Result<(), Unopenable> {
        if names.is_empty() {
            return Ok(());
        }
        let servers = self.list_servers().await.map_err(Unopenable::Unread)?;
        for name in names {
            if name.trim().is_empty() {
                return Err(Unopenable::Name(
                    "--allow-mcp needs a server's name; it was given a blank one".to_owned(),
                ));
            }
            if name == RESERVED_SERVER_NAME {
                return Err(Unopenable::Name(format!(
                    "'{name}' is gglib's own tools, not an MCP server, and is never opened by name"
                )));
            }
            if the_one(&servers, name).is_none() {
                let shared = servers.iter().filter(|s| &s.name == name).count();
                return Err(Unopenable::Name(if shared > 1 {
                    format!("{shared} MCP servers are named '{name}'; rename one, then name it")
                } else {
                    format!("no MCP server is named '{name}'; `gglib mcp list` shows their names")
                }));
            }
        }
        Ok(())
    }

    /// The tools of the servers `names` opens, as the agent loop names them
    /// (`{id}:{tool}`), each named server started first when it is not
    /// running.
    ///
    /// A name that opens no one server, a server that is disabled, invalid,
    /// manual and stopped, or that cannot be started is skipped with one
    /// warning that names it and says why, and the rest are offered. The why
    /// can hold what a server that failed to start printed on stderr, as the
    /// daemon's own start warning does; never a tool's arguments or results.
    /// Nothing here fails the run: a run offered fewer tools still answers.
    pub async fn opened_tools(&self, names: &[String]) -> Vec<String> {
        if names.is_empty() {
            return Vec::new();
        }
        let servers = match self.list_servers().await {
            Ok(servers) => servers,
            Err(e) => {
                warn!(error = %e, "could not read the MCP servers a paired device's run is offered");
                return Vec::new();
            }
        };
        let mut offered = Vec::new();
        for name in names {
            let Some(server) = the_one(&servers, name) else {
                warn!(server = %name, "no one MCP server has this opened name; it offers no tool");
                continue;
            };
            if let Err(why) = self.started(server).await {
                warn!(server = %name, reason = %why, "an opened MCP server is not running; it offers no tool");
                continue;
            }
            match self.list_server_tools(server.id).await {
                Ok(tools) => offered.extend(tools.iter().map(|t| qualified(server.id, &t.name))),
                Err(e) => {
                    warn!(server = %name, reason = %e, "an opened MCP server's tools could not be listed");
                }
            }
        }
        offered
    }

    /// `server` running, started as a tool call would start it when it is
    /// not: its executable resolved first, as `prewarm_lazy`'s start
    /// resolves it, then the start the per-server lock keeps to one.
    async fn started(&self, server: &McpServer) -> Result<(), String> {
        if !server.enabled {
            return Err("it is disabled".to_owned());
        }
        if !server.is_valid {
            return Err(server
                .last_error
                .clone()
                .unwrap_or_else(|| "its configuration is invalid".to_owned()));
        }
        if self.get_server_status(server.id).await == McpServerStatus::Running {
            return Ok(());
        }
        if server.server_type == McpServerType::Stdio {
            let resolved = self
                .ensure_resolved(server.id)
                .await
                .map_err(|e| e.to_string())?;
            if !resolved.success {
                return Err(resolved.error_message_with_suggestions());
            }
        }
        self.ensure_started_for_call(server.id)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "opened_tests.rs"]
mod opened_tests;
