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

## Internal Structure

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
│                                                             │
│  resolver/ (path resolution)                                │
│    ├── types: Result/Error/Attempt types                    │
│    ├── env: Environment variable access (injectable)        │
│    ├── fs: Filesystem operations (injectable)               │
│    ├── search: Platform-specific search strategies          │
│    └── resolve: 6-step resolution orchestration             │
│                                                             │
│  path: Validation & PATH building utilities                 │
└─────────────────────────────────────────────────────────────┘
                           │
           depends on (no sqlx!)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                      gglib-core                             │
│  domain/mcp/types.rs  │  ports/mcp_repository.rs            │
│  events/mcp.rs        │  ports/event_emitter.rs             │
│  ports/mcp_dto.rs     │  (DTOs for Tauri/Axum/TypeScript)   │
└─────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### No `SQLx` Dependency

This crate has **no direct database dependency**. Persistence is injected via the `McpServerRepository` trait from `gglib-core::ports`. This keeps the crate focused on:
- MCP protocol handling
- Server process lifecycle
- Tool discovery and invocation

The `SQLite` implementation lives in `gglib-db` (`SqliteMcpRepository`).

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

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`client.rs`](src/client.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-client-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-client-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-client-coverage.json) |
| [`manager.rs`](src/manager.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-manager-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-manager-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-manager-coverage.json) |
| [`path.rs`](src/path.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-path-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-path-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-path-coverage.json) |
| [`service.rs`](src/service.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-service-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-service-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-service-coverage.json) |
| [`resolver/`](src/resolver/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-resolver-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`client.rs`** — Low-level JSON-RPC 2.0 client for the MCP protocol
- **`manager.rs`** — Server process lifecycle management (start/stop/status)
- **`service.rs`** — High-level facade for MCP operations (CRUD + lifecycle)
- **`path.rs`** — Path validation and PATH environment variable utilities
- **`resolver/`** — Cross-platform executable path resolution with 6-step search strategy


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
- Executable path resolution with caching and diagnostics

### `resolver` Module

Cross-platform executable path resolution:
- **6-step search strategy**: Absolute path → PATH → /etc/paths (macOS) → Platform defaults → Node managers → User paths
- **Trait-based testing**: Injectable `EnvProvider` and `FsProvider` for mocking
- **Diagnostic output**: Detailed attempt log for troubleshooting path issues
- **Platform-aware**: Handles macOS Finder minimal PATH, Windows PATHEXT, Node version managers
- **Fallback behavior**: Tries basename search if absolute path fails

See [`resolver/mod.rs`](src/resolver/mod.rs) for detailed documentation and usage examples.

### `path` Module

Path validation and environment utilities:
- **Validation functions**: Check executable paths, working directories
- **PATH building**: Construct effective PATH from user paths and executable directory
- **De-duplication**: Ensures no duplicate entries in PATH

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
