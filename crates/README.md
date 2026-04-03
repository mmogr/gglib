# gglib Crates

This directory contains all the crates that make up gglib's modular architecture.

## Architecture Overview

gglib follows **hexagonal architecture** (ports & adapters) with clear separation of concerns:

```text
┌────────────────────────────────────────────────────────────────────────────┐
│                            ADAPTER LAYER                                   │
│       ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                 │
│       │  gglib-cli   │  │ gglib-axum   │  │ gglib-tauri  │                 │
│       │  CLI tool    │  │  REST API    │  │Tauri backend │                 │
│       └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                 │
└──────────────┼─────────────────┼─────────────────┼─────────────────────────┘
               └─────────────────┼─────────────────┘
                                 │
┌────────────────────────────────────────────────────────────────────────────┐
│                            FACADE LAYER                                    │
│                        ┌──────────────┐                                    │
│                        │  gglib-gui   │                                    │
│                        │Shared UI core│                                    │
│                        └──────┬───────┘                                    │
└───────────────────────────────┼────────────────────────────────────────────┘
                                │
┌────────────────────────────────────────────────────────────────────────────┐
│                             CORE LAYER                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                        gglib-core                                    │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐   │  │
│  │  │  domain/   │  │  ports/    │  │ services/  │  │  events/   │   │  │
│  │  │Pure types  │  │  Traits    │  │ Use cases  │  │  Events    │   │  │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘   │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────┘
                                   │
┌────────────────────────────────────────────────────────────────────────────┐
│                          APPLICATION LAYER                                 │
│                        ┌──────────────────┐                                │
│                        │   gglib-agent    │                                │
│                        │  Agentic loop    │                                │
│                        │ (pure domain,    │                                │
│                        │  port-injected)  │                                │
│                        └────────┬─────────┘                                │
└─────────────────────────────────┼──────────────────────────────────────────┘
                                   │
                  ┌────────────────┼────────────────┐
                  │                │                │
┌─────────────────────────────────────────────────────────────────────────────┐
│                        INFRASTRUCTURE LAYER                                 │
│                                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │
│  │  gglib-db   │  │ gglib-gguf  │  │  gglib-hf   │  │ gglib-mcp   │      │
│  │  SQLite     │  │GGUF parsing │  │HuggingFace  │  │   MCP SDK   │      │
│  │repositories │  │             │  │   client    │  │             │      │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘      │
│                                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │
│  │gglib-runtime│  │gglib-download│  │gglib-proxy  │  │gglib-voice  │      │
│  │llama.cpp    │  │   Download   │  │OpenAI proxy │  │ Voice mode  │      │
│  │ management  │  │   manager    │  │             │  │ STT/TTS/VAD │      │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Dependency Flow

```text
Adapter Layer
    ↓
Facade Layer (gglib-gui)
    ↓
Core Layer (gglib-core)    Core Layer (gglib-core)
    ↓                              ↓
Application Layer          Infrastructure Layer
(gglib-agent)
```

**Key Principle**: Both the Application layer and the Infrastructure layer depend on
`gglib-core` (via port traits), never the reverse. Adapters wire them together at
the composition root.

## Crate Catalog

### Core Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-core](gglib-core/)** | Pure domain types, port traits, and application services. No infrastructure dependencies. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json) |

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
| **[gglib-voice](gglib-voice/)** | Voice mode pipeline with local STT (Whisper), TTS (Kokoro), and VAD (Silero) via sherpa-onnx. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-voice-loc.json) |

### Facade Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-gui](gglib-gui/)** | Shared business logic for GUI applications (ensures feature parity). | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gui-loc.json) |

### Adapter Layer

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-cli](gglib-cli/)** | Command-line interface for all gglib operations. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json) |
| **[gglib-axum](gglib-axum/)** | REST API server built with Axum for web/GUI clients. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json) |
| **[gglib-tauri](gglib-tauri/)** | Tauri backend for desktop application. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-loc.json) |

### Utility Crates

| Crate | Purpose | Lines of Code |
|-------|---------|---------------|
| **[gglib-build-info](gglib-build-info/)** | Compile-time version and git metadata for CLI/GUI version strings. | ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json) |

## Crate Responsibilities

### Core Layer: gglib-core

**What it contains:**
- Domain models (`Model`, `Conversation`, `McpServer`)
- Port trait definitions (interfaces for infrastructure)
- Application services (business logic orchestration)
- Event types for cross-layer communication

**What it DOES NOT contain:**
- Database code
- HTTP clients
- File I/O
- Process management
- Any infrastructure concerns

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
- `ModelRepository` → `SqlxModelRepository`
- `McpRepository` → `SqlxMcpRepository`
- `ChatHistoryRepository` → `SqlxChatHistoryRepository`
- `SettingsRepository` → `SqlxSettingsRepository`

#### gglib-runtime
Manages llama.cpp lifecycle:
- Installation and updates
- Configuration and argument building
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

#### gglib-voice
Voice mode pipeline:
- Audio capture (cpal) with resampling to 16 kHz mono
- Speech-to-text via sherpa-onnx (Whisper ONNX, 7 model sizes)
- Text-to-speech via sherpa-onnx (Kokoro v0.19, 11 voices)
- Voice activity detection (Silero neural-net VAD + energy fallback)
- Echo gate to suppress mic during TTS playback
- Pipeline state machine orchestrating the full conversation loop
- Safe audio threading via actor pattern (no unsafe code)

### Adapter Layer

#### gglib-cli
Command-line interface:
- `gglib model add` - Add models
- `gglib model list` - List models
- `gglib serve` - Start servers
- `gglib chat` - Interactive chat
- `gglib config` - Configuration management

#### gglib-axum
REST API endpoints:
- `POST /api/models` - Add model
- `GET /api/models` - List models
- `POST /api/serve/:id` - Start server
- `POST /api/chat/completions` - Chat endpoint
- `GET /api/events` - SSE event stream

#### gglib-gui
Shared GUI logic:
- Backend service orchestration
- State management
- Event handling
- Business logic for UI operations

#### gglib-tauri
Desktop application backend:
- Tauri command handlers
- Window management
- Native OS integration
- IPC with frontend

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

- **Core crate**: Only standard library + serde + async-trait
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