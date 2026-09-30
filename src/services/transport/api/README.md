# api

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-api-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-api-complexity.json)

<!-- module-docs:start -->

HTTP API transport implementations for all backend domains. Wraps `fetch` with automatic bearer-token injection, retry logic on 401/network failures, and API session discovery in Tauri environments. `createApiTransport()` spreads these modules into the object `getTransport()` returns — the models pair through `createModelsApi()`, the rest as namespaces.

## Request Flow

```
ChatPage.tsx: getTransport().listConversations()
       ▼
transport/api/chat.ts
       ▼
client.ts  ─── injects Authorization header
           ─── retries on 401 (re-fetches session token)
           ─── throws TransportError on failure
       ▼
Axum backend (HTTP response) → typed result
```

## Key Files

| File | Role |
|------|------|
| `client.ts` | HTTP client with auth injection, retry, and error normalization |
| `daemonToken.ts` | The daemon's token from the link `gglib web` prints: kept for the tab, stripped from the address bar |
| `chat.ts` | Conversations and messages |
| `servers.ts` | llama.cpp server lifecycle and proxy |
| `downloads.ts` | Download queue management |
| `mcp.ts` | MCP server config and tool execution |
| `settings.ts` | Application settings |
| `tags.ts` | Model tags |
| `builtin.ts` | Built-in tool listing |
| `verification.ts` | Model verification |
| `proxy.ts` | OpenAI-compatible proxy management |
| `remote.ts` | The remote tunnel (ADR 0012): enable/disable/status here, join/disconnect/kill for another machine |
| `models/` | Local and HuggingFace model APIs |
| `setup.ts` | First-run setup and dependency probes |
| `sse.ts` | The server-sent-events endpoint this transport subscribes to |
| `runs.ts` | Runs (`/api/runs`): start an agent run under a minted id, list, cancel, and read its events from any point |
| `sseEvents.ts` | Reading SSE events, with their `id:` and `event:` fields, off a `fetch` response |
| `version.ts` | Which build of gglib the daemon is running |

<!-- module-docs:end -->
