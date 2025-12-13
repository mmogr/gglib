# gglib-mcp

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-complexity.json)

MCP (Model Context Protocol) server management for gglib.

## Architecture

This crate is in the **Infrastructure Layer** — it manages MCP server lifecycle and protocol handling.

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Overview

This crate provides MCP server lifecycle management, including:
- **JSON-RPC 2.0 protocol client** for communicating with MCP servers
- **Server lifecycle management** (start, stop, status tracking)
- **Tool discovery and invocation** via the MCP protocol

## Architecture

```text
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

```rust,no_run
use std::sync::Arc;
use gglib_mcp::McpService;
use gglib_core::ports::{McpServerRepository, AppEventEmitter, NoopEmitter};

async fn example(sqlite_mcp_repo: impl McpServerRepository + 'static) {
    let service = McpService::new(
        Arc::new(sqlite_mcp_repo),  // McpServerRepository
        Arc::new(NoopEmitter),      // AppEventEmitter (or TauriEmitter)
    );
}
```

This enables:
- Easy testing with mock repositories
- Different event emitters per adapter (Tauri events vs CLI noop)
- Clean separation of concerns

## Components

<details>
<summary><h2>Modules</h2></summary>

| Module | LOC | Complexity | Coverage | Tests |
|--------|-----|------------|----------|-------|
| [`client.rs`](src/client.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-client-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-client-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-client-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-client-tests.json) |
| [`manager.rs`](src/manager.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-manager-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-manager-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-manager-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-manager-tests.json) |
| [`service.rs`](src/service.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-service-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-service-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-service-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-service-tests.json) |

</details>

**Module Descriptions:**
- **`client.rs`** — Low-level JSON-RPC 2.0 client for the MCP protocol
- **`manager.rs`** — Server process lifecycle management (start/stop/status)
- **`service.rs`** — High-level facade for MCP operations (CRUD + lifecycle)


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

```rust,no_run
use std::sync::Arc;
use gglib_mcp::McpService;
use gglib_core::ports::{McpServerRepository, AppEventEmitter, NoopEmitter};
use gglib_core::domain::mcp::{NewMcpServer, McpServerType, McpServerConfig};

async fn example(repo: impl McpServerRepository + 'static) {
    // Create service with injected dependencies
    let service = McpService::new(Arc::new(repo), Arc::new(NoopEmitter));

    // Add a server configuration
    let server = service.add_server(NewMcpServer {
        name: "my-mcp-server".to_string(),
        server_type: McpServerType::Stdio,
        config: McpServerConfig::stdio(
            "npx",
            vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()],
            None,
        ),
        enabled: true,
        auto_start: false,
        env: vec![],
    }).await.unwrap();

    // Start the server
    service.start_server(server.id).await.unwrap();

    // List available tools from all running servers
    let tools = service.list_all_tools().await;

    // Stop the server
    service.stop_server(server.id).await.unwrap();
}
```

## Testing

The crate uses trait-based testing. See `gglib-db` for `SqliteMcpRepository` unit tests (8 tests covering all CRUD operations).
