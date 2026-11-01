//! MCP server management subcommands.
//!
//! This module defines the MCP server lifecycle and configuration commands.

use clap::Subcommand;

/// MCP server management commands.
#[derive(Subcommand)]
pub enum McpCommand {
    /// List all configured MCP servers, with each one's status in the
    /// daemon when it is running
    List,

    /// Add a new MCP server
    Add {
        /// Server display name
        #[arg(long)]
        name: String,

        /// Server type: "stdio" (a process gglib starts). "sse" (HTTP) is not
        /// supported yet, and is refused
        #[arg(long, value_name = "TYPE")]
        r#type: String,

        /// Command to run (stdio only)
        #[arg(long)]
        command: Option<String>,

        /// Arguments for the command (stdio only, comma-separated e.g. '--port,3000')
        #[arg(long, value_delimiter = ',')]
        args: Vec<String>,

        /// URL of an sse server (not supported yet)
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

        /// Lifecycle policy: how gglib manages this server's process.
        /// eager = start at host init; lazy = start on first tool use.
        #[arg(long, value_name = "LIFECYCLE", default_value = "lazy", value_parser = ["eager", "lazy"])]
        lifecycle: String,

        /// Disable this server (tools not included in chat)
        #[arg(long)]
        disabled: bool,
    },

    /// Remove an MCP server
    Remove {
        /// Server ID or name
        server: String,

        /// Answer yes to the confirmation, so nothing is asked
        #[arg(short = 'y', long)]
        yes: bool,
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

    /// Open MCP servers to paired devices: their runs are offered the
    /// servers' tools, from their next run on
    AllowRemote {
        /// Server IDs or names
        #[arg(required = true)]
        servers: Vec<String>,
    },

    /// Close MCP servers to paired devices
    DenyRemote {
        /// Server IDs or names
        #[arg(required = true)]
        servers: Vec<String>,
    },

    /// List the tools the daemon's copy of a server offers its chats,
    /// starting it there if it is not running
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
