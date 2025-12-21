# domain

<!-- module-docs:start -->

Pure domain types representing the core business entities.

These types form the heart of the hexagonal architecture — they have **no infrastructure dependencies** and can be used throughout the application without coupling to databases, filesystems, or external services.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              domain/                                                │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌──────────────────────────────────────────────────────────────────────────────┐   │
│  │                         Entity Types                                         │   │
│  │  Model, ModelFilterOptions, NewModel                                         │   │
│  │  Conversation, Message, NewConversation, NewMessage                          │   │
│  └──────────────────────────────────────────────────────────────────────────────┘   │
│                                      │                                              │
│                                      ▼                                              │
│  ┌──────────────────────────────────────────────────────────────────────────────┐   │
│  │                         MCP Types (mcp/)                                     │   │
│  │  McpServer, McpServerConfig, McpTool, McpServerStatus                        │   │
│  └──────────────────────────────────────────────────────────────────────────────┘   │
│                                      │                                              │
│                                      ▼                                              │
│  ┌──────────────────────────────────────────────────────────────────────────────┐   │
│  │                         GGUF Types                                           │   │
│  │  GgufMetadata, GgufCapabilities, CapabilityFlags                             │   │
│  └──────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Key Types

| Type | Description |
|------|-------------|
| `Model` | A persisted GGUF model with metadata, path, and HuggingFace provenance |
| `NewModel` | Data for inserting a new model (no ID yet) |
| `Conversation` | A chat conversation with title, model reference, and system prompt |
| `Message` | A single chat message with role (system/user/assistant) |
| `McpServer` | An MCP server configuration with connection details |
| `GgufCapabilities` | Detected model capabilities (reasoning, tool-calling, vision) |

## Design Principles

1. **Serializable** — All types derive `Serialize`/`Deserialize` for JSON transport
2. **Cloneable** — Types are cheap to clone for passing between layers
3. **Infrastructure-Free** — No database types, no filesystem types
4. **New/Entity Pattern** — `NewX` types for insertion, `X` for persisted entities

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`chat.rs`](chat) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-chat-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-chat-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-chat-coverage.json) |
| [`gguf.rs`](gguf) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-gguf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-gguf-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-gguf-coverage.json) |
| [`model.rs`](model) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-model-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-model-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-model-coverage.json) |
| [`mcp/`](mcp/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-mcp-coverage.json) |
<!-- module-table:end -->

</details>
