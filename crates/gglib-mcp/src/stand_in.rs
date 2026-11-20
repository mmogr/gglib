//! An MCP server for a test to start, the same program on every OS.
//!
//! A test that starts a server through [`McpService`](crate::McpService), or
//! adds one with `gglib mcp add`, needs a program that answers the two
//! requests a start sends. [`StandIn::install`] puts one in a folder of its
//! own, and [`StandIn::server`] is a row that starts it.
//!
//! The program is `gglib-test-proc` following three instructions: print the
//! reply to `initialize`, print the reply to `tools/list`, then read its
//! input until it ends. So it stops the moment a client closes its input,
//! and a stop never waits out [`SERVER_EXIT_GRACE`](crate::SERVER_EXIT_GRACE).
//!
//! It answers before it is asked. The two replies carry the ids 1 and 2,
//! which are the ids of the two requests a start sends, in that order, on a
//! client that has sent nothing else. A client that sent any other request
//! before `tools/list` would number that one 2 and wait for a reply to 3
//! that never comes: every start of a stand-in would fail at the client's
//! reply timeout. Its one tool, `echo`, is listed and cannot be called: a
//! call is never answered.

use std::path::{Path, PathBuf};

use gglib_core::NewMcpServer;

/// The name the program is installed under, and the command a row names it
/// by. No other program has it, so only the folder a row adds can resolve it.
const COMMAND: &str = "gglib-stand-in-mcp-server";

/// What the program does: see the module's own words.
const INSTRUCTIONS: &str = concat!(
    r#"print {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"stand-in"}}}"#,
    "\n",
    r#"print {"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo","description":"says it back"}]}}"#,
    "\n",
    "read",
);

/// A stand-in MCP server, installed in a temporary folder that is removed
/// when this is dropped. Keep it until every server started from it has
/// stopped: Windows will not remove a program that is running.
pub struct StandIn {
    folder: tempfile::TempDir,
    program: PathBuf,
}

impl StandIn {
    /// Install the program in a new temporary folder.
    ///
    /// # Panics
    ///
    /// When the folder cannot be made or the program cannot be installed
    /// (`gglib_test_proc::install` says why).
    #[must_use]
    pub fn install() -> Self {
        let folder = tempfile::tempdir().expect("a temporary folder for the stand-in MCP server");
        let program = gglib_test_proc::install(folder.path(), COMMAND, INSTRUCTIONS);
        Self { folder, program }
    }

    /// A server named `name` that starts the program, as `gglib mcp add`
    /// stores one: lazy, enabled, its command a bare name and no path
    /// resolved yet. The row's extra search path is the program's folder, so
    /// a start resolves the command there, to [`Self::program`].
    ///
    /// Any number of rows can start the one program, each as a process of
    /// its own.
    #[must_use]
    pub fn server(&self, name: &str) -> NewMcpServer {
        let folder = self.folder.path().to_string_lossy().into_owned();
        NewMcpServer::new_stdio(name, COMMAND, Vec::new(), Some(folder))
    }

    /// The installed program: what a row's command resolves to, with `.exe`
    /// on Windows. A test that starts the server some other way starts this.
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }
}
