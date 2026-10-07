# transport

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-complexity.json)

<!-- module-docs:start -->

The core platform abstraction layer. Provides `getTransport()` — a factory that spreads the HTTP API client and the SSE event bus into one instance, memoised after the first call. Its type is inferred from those two factories rather than restated as an interface, so it cannot drift from what they return.

There is no branch **between transports**: desktop and web both talk to the gglib daemon over HTTP+SSE. Platform does still matter inside `api/client.ts`, which asks `isDesktop()` (`platform/detect.ts`, the one check) to resolve the daemon's base URL through the `get_embedded_api_info` IPC command and to pick the retry path. That is the layer's job — absorbing the difference so callers never see it.

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
                One object, spread from both
                (its type inferred from the two)
```

Both halves reach the same gglib daemon, on desktop and on the web.

## Subdirectories

| Directory | Role |
|-----------|------|
| `types/` | Per-domain wire shapes, what each call takes, and ID aliases |
| `api/` | HTTP API implementations (one module per domain) |
| `events/` | Real-time event subscriptions over SSE |

## Key Files

| File | Role |
|------|------|
| `index.ts` | `getTransport()` factory; spreads the API client and event bus into one object, then memoises |
| `errors.ts` | `TransportError` with typed error codes (`NOT_SUPPORTED`, `NETWORK_ERROR`, etc.) |
| `mappers.ts` | Maps frontend types to backend request DTOs (`toStartServerRequest()`, etc.) |
| `sanitizeMessages.ts` | Strips `<think>` tags and unsupported fields before sending to llama-server |
| `parseTitleResponse.ts` | Parses LLM title generation responses |
| `slotTokens.ts` | Tokens in use across a proxy's inference slots |

<!-- module-docs:end -->
