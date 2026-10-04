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
| `renew.ts` | A stream refused with a 401 renews the session's credential before it reconnects |
| `daemonToken.ts` | The daemon's token from the link `gglib web` prints: stripped from the address bar, kept until the daemon next starts |
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
| `farChats.ts` | The far machine's chats and runs (`/api/remote/chats`, `/api/remote/runs`), forwarded by this machine's daemon: list, open, add a turn of text and images (by the ids the far store answered), and read, list and cancel its runs |
| `attachments.ts` | The image stores (`/api/attachments`, and the far machine's at `/api/remote/attachments` for a far chat): upload an image as its raw bytes, and read one's bytes back with the page's credential |
| `farModels.ts` | The paired machine's models (`/api/remote/models`), read by this machine's daemon: the list with that machine and what may be done there, one model by its id there, and a load |
| `sseEvents.ts` | Reading SSE events, with their `id:` and `event:` fields, off a `fetch` response |
| `version.ts` | Which build of gglib the daemon is running |

<!-- module-docs:end -->
