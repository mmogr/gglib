# transport

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-complexity.json)

<!-- module-docs:start -->

The core platform abstraction layer. Defines the unified `Transport` interface that composes all domain sub-interfaces and provides `getTransport()` — a factory that detects the platform at startup and returns either a Tauri IPC or HTTP+SSE implementation. All code outside this directory is completely platform-agnostic.

## Architecture

```
             getTransport()   ← singleton, cached after first call
                   ▼
          Platform detection
          ┌────────────────────┐
          │   isTauri()?       │
          └──────┬──────┬──────┘
                 │      │
           Yes   │      │  No
                 ▼      ▼
          ┌─────────┐  ┌──────────────┐
          │  Tauri  │  │ HTTP + SSE   │
          │  IPC    │  │  transport   │
          └─────────┘  └──────────────┘
                 │      │
                 └──────┘
                    ▼
          Unified Transport object
          (satisfies all *Transport interfaces)
```

## Subdirectories

| Directory | Role |
|-----------|------|
| `types/` | Interface definitions — the contract all implementations must satisfy |
| `api/` | HTTP API implementations (one module per domain) |
| `events/` | Real-time event subscriptions (Tauri events or SSE) |
| `platform/` | Platform-specific operations (llama install, URL open, file dialogs) |

## Key Files

| File | Role |
|------|------|
| `index.ts` | `getTransport()` factory; platform detection and composition |
| `errors.ts` | `TransportError` with typed error codes (`NOT_SUPPORTED`, `NETWORK_ERROR`, etc.) |
| `mappers.ts` | Maps frontend types to backend request DTOs (`toStartServerRequest()`, etc.) |
| `sanitizeMessages.ts` | Strips `<think>` tags and unsupported fields before sending to llama-server |
| `parseTitleResponse.ts` | Parses LLM title generation responses |

<!-- module-docs:end -->
