# gglib Crates

This directory holds every crate of the workspace except the desktop app, which is `src-tauri`. The diagram below is the workspace's layer diagram: the root README and CONTRIBUTING link to it and draw none of their own. CONTRIBUTING's [Crate Boundaries](../CONTRIBUTING.md#crate-boundaries) holds the dependency rules and says which of them CI checks.

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

**Key Principle**: no crate depends on a layer above its own, and `gglib-core`
depends on no other gglib crate. Infrastructure crates such as `gglib-db`
implement its port traits, and `gglib-cli` and `gglib-axum` wire them together
through `gglib-bootstrap`.

## Crate Catalog

Each row links the crate, whose own README says what it is responsible for and how it is laid out.

### Core Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-core](gglib-core/)** | Domain types, port traits, application services and the event types adapters are notified with. Depends on no other gglib crate and on no database, HTTP or UI crate; it does local file I/O, and holds the helpers other crates build child-process commands with. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json) |

### Application Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-agent](gglib-agent/)** | Pure-domain agentic loop (LLM→tool→LLM cycle). Depends only on `gglib-core`. No HTTP, no MCP internals, no database. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loc.json) |

### Infrastructure Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-db](gglib-db/)** | SQLite repositories implementing `gglib-core` port traits for data persistence: `ModelRepository`, `McpServerRepository`, `ChatHistoryRepository` and `SettingsRepository` among them, each as a `Sqlite…` type. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json) |
| **[gglib-gguf](gglib-gguf/)** | GGUF file format parser: a model's architecture, quantization, context size and capabilities, and whether a file is a model's weights or a projector. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-loc.json) |
| **[gglib-hf](gglib-hf/)** | HuggingFace API client: model search, repository metadata, and the files and quantizations a repository holds. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-loc.json) |
| **[gglib-mcp](gglib-mcp/)** | Model Context Protocol server management: starting and stopping servers, the stdio JSON-RPC client, tool discovery and invocation, and the built-in tools. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-loc.json) |
| **[gglib-runtime](gglib-runtime/)** | llama.cpp installation and updates, launch-argument building (context size resolution included), process spawning and monitoring, health checks and port allocation. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-loc.json) |
| **[gglib-download](gglib-download/)** | Multi-file download manager with a queue, progress tracking, resume and cancel. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-loc.json) |
| **[gglib-proxy](gglib-proxy/)** | OpenAI-compatible proxy (`/v1/chat/completions`, `/v1/embeddings`, `/v1/models`, streaming included) with automatic model routing and swapping, and the MCP gateway at `/mcp`. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json) |

### Facade Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-app-services](gglib-app-services/)** | Backend facade shared by `gglib-axum`, `gglib-cli` and `src-tauri` (ensures feature parity). | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-loc.json) |
| **[gglib-bootstrap](gglib-bootstrap/)** | Composition root: `CoreBootstrap::build` wires the database, the download manager, the HuggingFace client and the GGUF parser once, for `gglib-cli` and `gglib-axum`. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-loc.json) |

### Adapter Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-cli](gglib-cli/)** | Command-line interface for all gglib operations; [docs/cli.md](../docs/cli.md) is the command reference. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json) |
| **[gglib-axum](gglib-axum/)** | The daemon's HTTP API, built with Axum for the web UI, the desktop app and the CLI. The router is built in [`src/routes.rs`](gglib-axum/src/routes.rs), which serves `/health` and nests the API under `/api`. The daemon paths the CLI calls are named in `gglib_core::contracts::http::daemon`, and `gglib-axum/tests/daemon_route_contract.rs` fails when the daemon stops serving one of them. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json) |
| **[gglib-tauri](gglib-tauri/)** | Tauri event-emission helpers (`emit_or_log` and the event names) for the desktop app. The Tauri commands, tray and menus live in `src-tauri` itself. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-loc.json) |

### Utility Crates

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-build-info](gglib-build-info/)** | Compile-time version and git metadata for CLI/GUI version strings. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json) |
| **[gglib-sse](gglib-sse/)** | Generic Server-Sent Events broadcast utility shared by `gglib-axum` and `gglib-proxy`. Zero `gglib-*` dependencies. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-sse-loc.json) |

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

What a crate in each layer may depend on, and which of those rules `scripts/check_boundaries.sh` checks, is in CONTRIBUTING's [Crate Boundaries](../CONTRIBUTING.md#crate-boundaries).

## Further Reading

- [Main README](../README.md) - Project overview and getting started
- [CONTRIBUTING](../CONTRIBUTING.md) - Conventions, the dependency rules, and the [Badges Pipeline](../CONTRIBUTING.md#badges-pipeline) that writes the LOC badges above
- Individual crate READMEs linked in table above
