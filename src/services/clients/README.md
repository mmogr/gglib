# clients

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-clients-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-clients-complexity.json)

<!-- module-docs:start -->

Domain-specific client facades exposing all backend capabilities to the React UI layer. Each module is a thin delegation wrapper over `getTransport()`, ensuring the UI never touches platform-specific code. All clients are platform-agnostic by design — transport selection happens once at composition root.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                 React Components                    │
└──────────────────────────┬──────────────────────────┘
                           ▼
┌─────────────────────────────────────────────────────┐
│            clients/  (Domain Facades)                │
│  chat.ts  models.ts  servers.ts  downloads.ts  ...  │
│             └── all call getTransport()             │
└──────────────────────────┬──────────────────────────┘
                           ▼
┌─────────────────────────────────────────────────────┐
│              transport/  (Platform Layer)            │
│         Tauri IPC  ──or──  HTTP + SSE               │
└─────────────────────────────────────────────────────┘
```

## Key Files

| File | Role |
|------|------|
| `chat.ts` | Conversations, messages, title generation |
| `models.ts` | Local model CRUD, search, HuggingFace browse |
| `downloads.ts` | Download queue management (queue, cancel, reorder) |
| `servers.ts` | llama.cpp server lifecycle and proxy operations |
| `settings.ts` | Application settings read/write |
| `mcp.ts` | MCP server configuration and tool execution |
| `events.ts` | Event subscription and unsubscribe |
| `builtin.ts` | Built-in tool registry access |
| `tags.ts` | Model tagging operations |
| `benchmark.ts` | Benchmark execution and result retrieval |
| `council.ts` | Council (multi-agent orchestrator) operations |
| `huggingface.ts` | HuggingFace Hub model discovery |
| `verification.ts` | Model/file verification |
| `system.ts` | System information and capability probes |

## Contract

Every export follows `clientFn(...) → getTransport().method(...)`. No client module may import from `platform/` — platform exceptions are handled inside `transport/platform/`.

<!-- module-docs:end -->
