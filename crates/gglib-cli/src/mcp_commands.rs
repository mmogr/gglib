//! MCP server management subcommands.
//!
//! This module defines the MCP server lifecycle and configuration commands.

use clap::Subcommand;

/// MCP server management commands.
#[derive(Subcommand)]
pub enum McpCommand {
    /// List all configured MCP servers with status
    List,

    /// Add a new MCP server
    Add {
        /// Server display name
        #[arg(long)]
        name: String,

        /// Server type: "stdio" (process) or "sse" (HTTP)
        #[arg(long, value_name = "TYPE")]
        r#type: String,

        /// Command to run (stdio only)
        #[arg(long)]
        command: Option<String>,

        /// Arguments for the command (stdio only, comma-separated e.g. '--port,3000')
        #[arg(long, value_delimiter = ',')]
        args: Vec<String>,

        /// URL to connect to (sse only)
        #[arg(long)]
        url: Option<String>,

        /// Working directory for the server process
        #[arg(long)]
        working_dir: Option<String>,

        /// Additional PATH entries (colon-separated)
        #[arg(long)]
        path_extra: Option<String>,

        /// Environment variables in KEY=VALUE format (can be repeated)
        #[arg(long, value_name = "KEY=VALUE")]
        env: Vec<String>,

        /// Auto-start this server when gglib launches
        #[arg(long)]
        auto_start: bool,

        /// Disable this server (tools not included in chat)
        #[arg(long)]
        disabled: bool,
    },

    /// Remove an MCP server
    Remove {
        /// Server ID or name
        server: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Start an MCP server
    Start {
        /// Server ID or name
        server: String,
    },

    /// Stop a running MCP server
    Stop {
        /// Server ID or name
        server: String,
    },

    /// Enable an MCP server (include tools in chat)
    Enable {
        /// Server ID or name
        server: String,
    },

    /// Disable an MCP server (exclude tools from chat)
    Disable {
        /// Server ID or name
        server: String,
    },

    /// List tools exposed by a running MCP server
    Tools {
        /// Server ID or name
        server: String,
    },

    /// Test connection to an MCP server (temporary start, list tools, stop)
    Test {
        /// Server ID or name
        server: String,
    },
}
