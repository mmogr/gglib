# transport

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-complexity.json)

<!-- module-docs:start -->

The core platform abstraction layer. Defines the unified `Transport` interface that composes all domain sub-interfaces and provides `getTransport()` — a factory that composes the HTTP API client with the SSE event bus into one instance, memoised after the first call.

There is no branch **between transports**: desktop and web both talk to the gglib daemon over HTTP+SSE. Platform does still matter inside `api/client.ts`, which uses its own local `isTauri()` to resolve the daemon's base URL through the `get_embedded_api_info` IPC command and to pick the retry path. That is the layer's job — absorbing the difference so callers never see it.

## Architecture

```
            getTransport()   ← singleton, cached after first call
                  ▼
   ┌──────────────────────┐   ┌──────────────────────┐
   │ createApiTransport() │   │   createEventBus()   │
   │      HTTP fetch      │   │         SSE          │
   └──────────┬───────────┘   └──────────┬───────────┘
              │                          │
              └────────────┬─────────────┘
                           ▼
                Unified Transport object
                (satisfies all *Transport interfaces)
```

Both halves reach the same gglib daemon, on desktop and on the web.

## Subdirectories

| Directory | Role |
|-----------|------|
| `types/` | Interface definitions — the contract all implementations must satisfy |
| `api/` | HTTP API implementations (one module per domain) |
| `events/` | Real-time event subscriptions over SSE |

## Key Files

| File | Role |
|------|------|
| `index.ts` | `getTransport()` factory; composes the API client and event bus, checks for key collisions, then memoises |
| `errors.ts` | `TransportError` with typed error codes (`NOT_SUPPORTED`, `NETWORK_ERROR`, etc.) |
| `utils.ts` | `checkCollisions()` — dev-mode guard against two modules exporting the same key |
| `mappers.ts` | Maps frontend types to backend request DTOs (`toStartServerRequest()`, etc.) |
| `sanitizeMessages.ts` | Strips `<think>` tags and unsupported fields before sending to llama-server |
| `parseTitleResponse.ts` | Parses LLM title generation responses |

<!-- module-docs:end -->
