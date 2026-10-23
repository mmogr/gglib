//! The MCP servers this machine opens to its paired devices, by name.
//!
//! `gglib remote enable --allow-mcp <server>` names them, and the names are
//! checked here, once, when the tunnel is enabled: a name that opens no one
//! server is refused then, so a typo cannot read as a grant. A name is the
//! one thing the owner types and sees, which is why the grant is by name and
//! not by the id the agent loop qualifies a tool with.

use gglib_core::{McpServer, McpServiceError};

use crate::service::{McpService, RESERVED_SERVER_NAME};

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
}

#[cfg(test)]
#[path = "opened_tests.rs"]
mod opened_tests;
