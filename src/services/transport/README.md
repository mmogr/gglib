# transport

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-complexity.json)

<!-- module-docs:start -->

The core platform abstraction layer. Defines the unified `Transport` interface that composes all domain sub-interfaces and provides `getTransport()` — a factory that composes the HTTP API client with the SSE event bus into one instance, memoised after the first call. There is no platform branch: desktop and web both talk to the gglib daemon over HTTP+SSE, the desktop WebView having discovered its base URL through `get_embedded_api_info`. All code outside this directory is completely platform-agnostic.

## Architecture

```
             getTransport()   ← singleton, cached after first call
                   ▼
          ┌──────────────────┐   ┌──────────────────┐
          │ createApiTransport() │ createEventBus() │
          │   HTTP fetch     │   │       SSE        │
          └────────┬─────────┘   └────────┬─────────┘
                   │                      │
                   └──────────┬───────────┘
                              ▼
                   Unified Transport object
                   (satisfies all *Transport interfaces)
```

Both halves talk to the same gglib daemon, on desktop and on the web. There
is no platform branch to pick between.

## Subdirectories

| Directory | Role |
|-----------|------|
| `types/` | Interface definitions — the contract all implementations must satisfy |
| `api/` | HTTP API implementations (one module per domain) |
| `events/` | Real-time event subscriptions over SSE |

## Key Files

| File | Role |
|------|------|
| `index.ts` | `getTransport()` factory; composes the API client and event bus, then memoises |
| `errors.ts` | `TransportError` with typed error codes (`NOT_SUPPORTED`, `NETWORK_ERROR`, etc.) |
| `mappers.ts` | Maps frontend types to backend request DTOs (`toStartServerRequest()`, etc.) |
| `sanitizeMessages.ts` | Strips `<think>` tags and unsupported fields before sending to llama-server |
| `parseTitleResponse.ts` | Parses LLM title generation responses |

<!-- module-docs:end -->
