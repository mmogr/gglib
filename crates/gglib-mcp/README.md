# gglib-mcp

MCP (Model Context Protocol) server management for gglib.

## Overview

This crate provides MCP server lifecycle management, including:
- **JSON-RPC 2.0 protocol client** for communicating with MCP servers
- **Server lifecycle management** (start, stop, status tracking)
- **Tool discovery and invocation** via the MCP protocol

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    gglib-mcp (this crate)                   │
├─────────────────────────────────────────────────────────────┤
│  McpService (facade)                                        │
│    ├── repo: Arc<dyn McpServerRepository>                   │
│    ├── emitter: Arc<dyn AppEventEmitter>                    │
│    └── manager: McpManager                                  │
│                                                             │
│  McpManager (lifecycle)                                     │
│    └── servers: HashMap<i64, RunningServer>                 │
│                                                             │
│  McpClient (protocol)                                       │
│    └── JSON-RPC 2.0, stdio transport                        │
└─────────────────────────────────────────────────────────────┘
                           │
           depends on (no sqlx!)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                      gglib-core                             │
│  domain/mcp/types.rs  │  ports/mcp_repository.rs            │
│  events/mcp.rs        │  ports/event_emitter.rs             │
└─────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### No SQLx Dependency

This crate has **no direct database dependency**. Persistence is injected via the `McpServerRepository` trait from `gglib-core::ports`. This keeps the crate focused on:
- MCP protocol handling
- Server process lifecycle
- Tool discovery and invocation

The SQLite implementation lives in `gglib-db` (`SqliteMcpRepository`).

### Dependency Injection

`McpService` accepts its dependencies via constructor injection:

```rust
let service = McpService::new(
    Arc::new(sqlite_mcp_repo),  // McpServerRepository
    Arc::new(NoopEmitter),      // AppEventEmitter (or TauriEmitter)
);
```

This enables:
- Easy testing with mock repositories
- Different event emitters per adapter (Tauri events vs CLI noop)
- Clean separation of concerns

## Components

### `McpClient`

Low-level JSON-RPC 2.0 client for the MCP protocol:
- Connects to MCP servers via stdio
- Handles protocol initialization and capability negotiation
- Provides `list_tools()` and `call_tool()` methods

### `McpManager`

Manages running MCP server processes:
- Tracks active servers by ID
- Starts/stops server processes
- Maintains client connections for each server

### `McpService`

High-level facade combining persistence and lifecycle:
- CRUD operations for server configurations
- Server start/stop with event emission
- Tool listing and invocation across all running servers

## Usage

```rust
use gglib_mcp::McpService;
use gglib_core::ports::{McpServerRepository, AppEventEmitter, NoopEmitter};

// Create service with injected dependencies
let service = McpService::new(repo, Arc::new(NoopEmitter));

// Add a server configuration
let server = service.add_server(NewMcpServer {
    name: "my-mcp-server".to_string(),
    command: "npx".to_string(),
    args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()],
    env: vec![],
    server_type: McpServerType::Stdio,
}).await?;

// Start the server
service.start_server(server.id).await?;

// List available tools
let tools = service.list_tools(server.id).await?;

// Call a tool
let result = service.call_tool(server.id, "read_file", json!({"path": "/tmp/test.txt"})).await?;

// Stop the server
service.stop_server(server.id).await?;
```

## Testing

The crate uses trait-based testing. See `gglib-db` for `SqliteMcpRepository` unit tests (8 tests covering all CRUD operations).
