# api

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-api-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-api-complexity.json)

<!-- module-docs:start -->

HTTP API transport implementations for all backend domains. Wraps `fetch` with automatic bearer-token injection, retry logic on 401/network failures, and API session discovery in Tauri environments. Each domain module implements the corresponding `*Transport` interface from `transport/types/`.

## Request Flow

```
clients/chat.ts: listConversations()
       ▼
getTransport().listConversations()
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
| `chat.ts` | Conversations and messages |
| `servers.ts` | llama.cpp server lifecycle and proxy |
| `downloads.ts` | Download queue management |
| `mcp.ts` | MCP server config and tool execution |
| `settings.ts` | Application settings |
| `tags.ts` | Model tags |
| `builtin.ts` | Built-in tool listing |
| `verification.ts` | Model verification |
| `proxy.ts` | OpenAI-compatible proxy management |
| `models/` | Local and HuggingFace model APIs |

<!-- module-docs:end -->
