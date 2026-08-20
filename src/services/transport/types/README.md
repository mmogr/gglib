# types

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-types-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-types-complexity.json)

<!-- module-docs:start -->

All TypeScript types and DTOs forming the contract between the frontend and backend: branded ID types, common utility types, and per-domain request and response shapes. Most are re-exports of the ts-rs bindings under `src/types/generated/`.

## Key Files

| File | Role |
|------|------|
| `index.ts` | Barrel over the per-domain modules |
| `ids.ts` | Brand-tagged ID types: `ModelId`, `ConversationId`, `DownloadId`, `McpServerId`, etc. |
| `common.ts` | `Unsubscribe`, `EventHandler`, base error types |
| `models.ts` | Model shapes |
| `chat.ts` | `ConversationSummary`, `ChatMessage`, `SaveMessageParams` |
| `servers.ts` | `ServeConfig`, `ServerInfo`, `ServeResponse` |
| `downloads.ts` | Download queue types |
| `events.ts` | `ServerWireEvent`, `DownloadEvent`, `AppEventMap` |
| `settings.ts` | Application settings shapes |
| `mcp.ts` | MCP server and tool shapes |
| `verification.ts` | Model verification shapes |
| `proxy.ts` | Proxy status and configuration shapes |
| `dashboard.ts` | The proxy dashboard snapshot graph |
| `admission.ts` | Slot admission and residency shapes |

## Branded ID Types

IDs are branded (`ModelId`, `ConversationId`, etc.) to prevent accidental cross-domain ID substitution at compile time.

<!-- module-docs:end -->
