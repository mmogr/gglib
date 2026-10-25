//! The MCP servers this machine opens to its paired devices.
//!
//! Each server's own row says whether paired devices may use it
//! (`allow_remote`, ADR 0025), set by [`McpService::set_allow_remote`] from
//! `gglib mcp allow-remote`, the API and the GUI alike, and nothing else
//! keeps a list. A rename keeps the grant and a re-add starts closed, since
//! the grant is the row's.
//!
//! When a paired device's run starts, [`McpService::opened_tools`] turns the
//! open servers into the tools that run is offered, by the id each tool is
//! called by, starting an open server that is not running. That is the run's
//! own work, done once its model is ready, not the `PUT` that made it.

use gglib_core::{McpServer, McpServerStatus, McpServerType, McpServiceError};
use tracing::warn;

use crate::service::{McpService, RESERVED_SERVER_NAME};
use crate::tool_executor::qualified;

impl McpService {
    /// Open the server `id` to paired devices, or close it, and return the
    /// server as it now stands.
    ///
    /// The flag's own write, and nothing else: the server is not stopped,
    /// and its row is not rewritten whole, so a running server keeps running
    /// and a whole-row write made from an earlier read cannot undo it. A
    /// device's next run reads it.
    ///
    /// # Errors
    ///
    /// Opening an SSE server is refused with
    /// [`McpServiceError::SseNotSupported`], since gglib cannot run one, and
    /// opening one named `builtin` with [`McpServiceError::NeverOpened`].
    /// Closing a server is never refused. An unknown `id` is the
    /// repository's `NotFound`.
    pub async fn set_allow_remote(
        &self,
        id: i64,
        allow: bool,
    ) -> Result<McpServer, McpServiceError> {
        let mut server = self.get_server(id).await?;
        if allow {
            Self::refuse_opening(&server)?;
        }
        self.repository.set_allow_remote(id, allow).await?;
        server.allow_remote = allow;
        tracing::info!(
            server_name = %server.name,
            allow_remote = allow,
            "set whether paired devices may use an MCP server"
        );
        Ok(server)
    }

    /// Refuse to open `server` to paired devices when it never can be: an
    /// SSE server, which gglib cannot run, or `builtin`. A caller opening
    /// several asks this of each before it changes any.
    ///
    /// # Errors
    ///
    /// [`McpServiceError::SseNotSupported`] or [`McpServiceError::NeverOpened`].
    pub fn refuse_opening(server: &McpServer) -> Result<(), McpServiceError> {
        Self::refuse_unsupported(server.server_type)?;
        if server.name == RESERVED_SERVER_NAME {
            return Err(McpServiceError::NeverOpened(server.name.clone()));
        }
        Ok(())
    }

    /// The servers a paired device's runs are offered: each one flagged
    /// open, enabled, stdio, and not `builtin`, in the order they list.
    ///
    /// # Errors
    ///
    /// When the servers cannot be read.
    pub async fn open_to_devices(&self) -> Result<Vec<McpServer>, McpServiceError> {
        let mut servers = self.list_servers().await?;
        servers.retain(|server| {
            server.allow_remote
                && server.enabled
                && server.server_type == McpServerType::Stdio
                && server.name != RESERVED_SERVER_NAME
        });
        Ok(servers)
    }

    /// The names of [`Self::open_to_devices`], sorted and each once, for a
    /// status to show. None, with a warning, when the servers cannot be
    /// read: a status that cannot say is not a failure of the tunnel.
    pub async fn device_server_names(&self) -> Vec<String> {
        match self.open_to_devices().await {
            Ok(servers) => {
                let mut names: Vec<String> =
                    servers.into_iter().map(|server| server.name).collect();
                names.sort_unstable();
                names.dedup();
                names
            }
            Err(e) => {
                warn!(error = %e, "could not read the MCP servers open to paired devices");
                Vec::new()
            }
        }
    }

    /// The tools of the servers open to devices ([`Self::open_to_devices`]),
    /// as the agent loop names them (`{id}:{tool}`), each one started first
    /// when it is not running.
    ///
    /// A server that is invalid, manual and stopped, or that cannot be
    /// started is skipped with one warning that names it and says why, and
    /// the rest are offered. The why can hold what a server that failed to
    /// start printed on stderr, as the daemon's own start warning does;
    /// never a tool's arguments or results. Nothing here fails the run: a run
    /// offered fewer tools still answers.
    pub async fn opened_tools(&self) -> Vec<String> {
        let servers = match self.open_to_devices().await {
            Ok(servers) => servers,
            Err(e) => {
                warn!(error = %e, "could not read the MCP servers a paired device's run is offered");
                return Vec::new();
            }
        };
        let mut offered = Vec::new();
        for server in &servers {
            let name = &server.name;
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
