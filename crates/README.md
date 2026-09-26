# gglib Crates

This directory contains all the crates that make up gglib's modular architecture.

## Architecture Overview

gglib follows **hexagonal architecture** (ports & adapters) with clear separation of concerns:

```text
┌────────────────────────────────────────────────────────────────────────────┐
│                               ADAPTER LAYER                                │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐ │
│  │  gglib-cli   │   │  gglib-axum  │   │  src-tauri   │   │ gglib-tauri  │ │
│  │   CLI tool   │   │   REST API   │   │ desktop app  │   │ Tauri events │ │
│  └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘ │
└──────────────────────────────────────┬─────────────────────────────────────┘
                                       ▼
┌────────────────────────────────────────────────────────────────────────────┐
│                                FACADE LAYER                                │
│          ┌──────────────────────┐        ┌──────────────────────┐          │
│          │  gglib-app-services  │        │   gglib-bootstrap    │          │
│          │    shared backend    │        │   composition root   │          │
│          └──────────────────────┘        └──────────────────────┘          │
└──────────────────────────────────────┬─────────────────────────────────────┘
                                       ▼
┌────────────────────────────────────────────────────────────────────────────┐
│                            INFRASTRUCTURE LAYER                            │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐ │
│  │   gglib-db   │   │  gglib-gguf  │   │   gglib-hf   │   │  gglib-mcp   │ │
│  │    SQLite    │   │ GGUF parsing │   │ HuggingFace  │   │   MCP SDK    │ │
│  │ repositories │   │              │   │    client    │   │              │ │
│  └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘ │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐                    │
│  │gglib-runtime │   │gglib-download│   │ gglib-proxy  │                    │
│  │  llama.cpp   │   │   download   │   │ OpenAI proxy │                    │
│  │  management  │   │   manager    │   │              │                    │
│  └──────────────┘   └──────────────┘   └──────────────┘                    │
└──────────────────────────────────────┬─────────────────────────────────────┘
                                       ▼
┌────────────────────────────────────────────────────────────────────────────┐
│                             APPLICATION LAYER                              │
│                          ┌──────────────────────┐                          │
│                          │     gglib-agent      │                          │
│                          │     agentic loop     │                          │
│                          │   (port-injected)    │                          │
│                          └──────────────────────┘                          │
└──────────────────────────────────────┬─────────────────────────────────────┘
                                       ▼
┌────────────────────────────────────────────────────────────────────────────┐
│                       CORE LAYER AND UTILITY CRATES                        │
│  ┌────────────────────────────────────────────┐    ┌────────────────────┐  │
│  │                 gglib-core                 │    │  gglib-build-info  │  │
│  │    domain/  ports/  services/  events/     │    │     gglib-sse      │  │
│  │       paths/  normalize/  sse/  ...        │    │   utility crates   │  │
│  └────────────────────────────────────────────┘    └────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
```

An arrow means "depends on", and a crate may depend on any layer below its own,
not only the next: `gglib-cli` uses several infrastructure crates directly, and
`gglib-app-services` uses `gglib-agent`. Inside a layer, `gglib-cli` and
`src-tauri` depend on `gglib-axum`, which both use to host the daemon;
`src-tauri` depends on `gglib-tauri`; `gglib-download` depends on `gglib-hf`;
`gglib-proxy` depends on `gglib-mcp`; and `gglib-runtime` depends on `gglib-mcp`
and `gglib-proxy`. `gglib-build-info` and `gglib-sse` depend on no gglib crate.
`src-tauri` is the desktop app and lives outside `crates/`;
`gglib-integration-tests` holds cross-crate tests and has only dev-dependencies.

## Dependency Flow

```text
Adapter layer          gglib-cli, gglib-axum, src-tauri, gglib-tauri
    ↓
Facade layer           gglib-app-services, gglib-bootstrap
    ↓
Infrastructure layer   gglib-db, gglib-gguf, gglib-hf, gglib-mcp,
    ↓                  gglib-runtime, gglib-download, gglib-proxy
Application layer      gglib-agent
    ↓
Core layer             gglib-core; utility crates gglib-build-info, gglib-sse
```

**Key Principle**: no crate depends on a layer above its own, and `gglib-core`
depends on no other gglib crate. Infrastructure crates such as `gglib-db`
implement its port traits, and `gglib-cli` and `gglib-axum` wire them together
through `gglib-bootstrap`.

## Crate Catalog

### Core Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-core](gglib-core/)** | Domain types, port traits and application services. Depends on no other gglib crate and on no database, HTTP or UI crate; it does local file I/O. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json) |

### Application Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-agent](gglib-agent/)** | Pure-domain agentic loop (LLM→tool→LLM cycle). Depends only on `gglib-core`. No HTTP, no MCP internals, no database. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loc.json) |

### Infrastructure Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-db](gglib-db/)** | SQLite repositories implementing `gglib-core` port traits for data persistence. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json) |
| **[gglib-gguf](gglib-gguf/)** | GGUF file format parser for extracting model metadata and capabilities. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-loc.json) |
| **[gglib-hf](gglib-hf/)** | HuggingFace API client for model search and metadata retrieval. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-loc.json) |
| **[gglib-mcp](gglib-mcp/)** | Model Context Protocol SDK for managing MCP server lifecycle. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-loc.json) |
| **[gglib-runtime](gglib-runtime/)** | llama.cpp installation, configuration, and process management. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-loc.json) |
| **[gglib-download](gglib-download/)** | Multi-file download manager with queue, progress tracking, and resume capability. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-loc.json) |
| **[gglib-proxy](gglib-proxy/)** | OpenAI-compatible proxy with automatic model routing and swapping. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json) |

### Facade Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-app-services](gglib-app-services/)** | Backend facade shared by `gglib-axum`, `gglib-cli` and `src-tauri` (ensures feature parity). | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-loc.json) |
| **[gglib-bootstrap](gglib-bootstrap/)** | Composition root: `CoreBootstrap::build` wires the database, the download manager, the HuggingFace client and the GGUF parser once, for `gglib-cli` and `gglib-axum`. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-loc.json) |

### Adapter Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-cli](gglib-cli/)** | Command-line interface for all gglib operations. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json) |
| **[gglib-axum](gglib-axum/)** | REST API server built with Axum for web/GUI clients. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json) |
| **[gglib-tauri](gglib-tauri/)** | Tauri event-emission helpers for the desktop app in `src-tauri`. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-loc.json) |

### Utility Crates

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-build-info](gglib-build-info/)** | Compile-time version and git metadata for CLI/GUI version strings. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json) |
| **[gglib-sse](gglib-sse/)** | Generic Server-Sent Events broadcast utility shared by `gglib-axum` and `gglib-proxy`. Zero `gglib-*` dependencies. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-sse-loc.json) |

## Crate Responsibilities

### Core Layer: gglib-core

**What it contains:**
- Domain models (`Model`, `Conversation`, `McpServer`)
- Port trait definitions (interfaces for infrastructure)
- Application services (business logic orchestration)
- Event types for cross-layer communication
- Local file I/O: data directories, the `.env` overrides file, device keys
- The helpers other crates build child-process commands with

**What it DOES NOT contain:**
- Database code
- HTTP clients or servers
- CLI, web or desktop UI frameworks

**Why this matters:**
- Keeps business logic pure and testable
- Enables infrastructure to be swapped without affecting logic
- Clear contracts via port traits

### Application Layer: gglib-agent

**What it contains:**
- `AgentLoop` — concrete `AgentLoopPort` implementation driving the ReAct-lite LLM→tool→LLM cycle
- Loop detection (FNV-1a batch-signature tracking, ported from TypeScript)
- Text stagnation detection
- Parallel tool execution with bounded concurrency and per-tool timeout
- Streaming response collection (forwards `TextDelta` events in real-time)
- Context budget pruning

**What it DOES NOT contain:**
- HTTP clients or any networking
- MCP SDK internals
- Database access
- Any reference to specific adapter/infrastructure crates

**Why this matters:**
- The full agentic loop runs as pure Rust domain logic, fully unit-testable with mocks
- Concrete `LlmCompletionPort` and `ToolExecutorPort` implementations are injected at composition root
- Port-parity with the TypeScript frontend ensures consistent behaviour across transports

### Infrastructure Layer

#### gglib-db
Implements port traits for data persistence:
- `ModelRepository` → `SqliteModelRepository`
- `McpServerRepository` → `SqliteMcpRepository`
- `ChatHistoryRepository` → `SqliteChatHistoryRepository`
- `SettingsRepository` → `SqliteSettingsRepository`

#### gglib-runtime
Manages llama.cpp lifecycle:
- Installation and updates
- Configuration and argument building
- Context size resolution (explicit flag → per-model server defaults → settings default → fitted to this machine → built-in floor)
- Process spawning and monitoring
- Health checking
- Port allocation

#### gglib-download
Handles model file downloads:
- Multi-file concurrent downloads
- Progress tracking and reporting
- Pause/resume/cancel
- Queue management
- Retry logic with exponential backoff

#### gglib-proxy
OpenAI-compatible HTTP proxy:
- `/v1/chat/completions` endpoint
- Automatic model routing
- Model swapping for load balancing
- Streaming support

#### gglib-gguf
Parses GGUF files to extract:
- Model architecture
- Quantization method
- Context size
- Capabilities (tool calling, vision, etc.)

#### gglib-hf
Interacts with HuggingFace:
- Search models by name/tags
- Retrieve model metadata
- List model files
- Check file availability

#### gglib-mcp
Model Context Protocol integration:
- Start/stop MCP servers
- Manage stdio communication
- Track server health
- Tool discovery

### Adapter Layer

#### gglib-cli
Command-line interface:
- `gglib model add` - Add models
- `gglib model list` - List models
- `gglib serve` - Pinned single-model OpenAI endpoint
- `gglib chat` - Interactive chat
- `gglib config` - Configuration management

#### gglib-axum
The daemon's HTTP API. The router is built in
[`src/routes.rs`](gglib-axum/src/routes.rs), which serves `/health` and nests
the API under `/api`. The daemon paths the CLI calls are named in
`gglib_core::contracts::http::daemon`, and
`gglib-axum/tests/daemon_route_contract.rs` fails when the daemon stops serving
one of them.

#### gglib-app-services
Backend facade shared by `gglib-axum`, `gglib-cli` and `src-tauri`:
- Backend service orchestration
- State management
- Event handling
- Business logic for UI operations

#### gglib-tauri
Event-emission helpers (`emit_or_log` and the event names) for the desktop
app. The Tauri commands, tray and menus live in `src-tauri` itself.

## Development Guidelines

### Adding a New Feature

1. **Define domain types** in `gglib-core/src/domain/`
2. **Define port trait** in `gglib-core/src/ports/` if infrastructure needed
3. **Implement service** in `gglib-core/src/services/` for business logic
4. **Implement adapter** in appropriate infrastructure crate
5. **Add presentation** in CLI/Axum/GUI as needed

### Testing Strategy

- **Unit tests**: Test services with mock ports
- **Integration tests**: Test adapters against real infrastructure
- **End-to-end tests**: Test full flow through CLI/API

### Adding Dependencies

- **Core crate**: No other gglib crate, and no database, HTTP or UI crate; `scripts/check_boundaries.sh` holds the names it rejects
- **Infrastructure crates**: Can depend on external services/libraries
- **Presentation crates**: Can depend on UI frameworks

## Further Reading

- [Main README](../README.md) - Project overview and getting started
- [Architecture Overview](../README.md#architecture) - Detailed architecture explanation
- Individual crate READMEs linked in table above

## Badge Information

All badges are generated via CI and stored in the `badges` branch. They reflect:
- **LOC**: Lines of code
- **Complexity**: Cyclomatic complexity
- **Coverage**: Test coverage percentage
- **Tests**: Test pass/fail status

See [`.github/workflows/badges.yml`](../.github/workflows/badges.yml) for badge generation logic.