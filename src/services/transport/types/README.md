# types

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-types-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-types-complexity.json)

<!-- module-docs:start -->

The transport's own TypeScript types: ID aliases, two event-handler types, what each call takes, and the shapes of the domains this layer owns (downloads, events, chat, MCP, the proxy, the remote tunnel, verification). Most are re-exports of the ts-rs bindings under `src/types/generated/`.

Model, server and settings shapes are in `src/types`, and are imported from there: neither barrel re-exports the other, so a type has one of the two as its home.

## Key Files

| File | Role |
|------|------|
| `index.ts` | Barrel over the per-domain modules |
| `ids.ts` | ID aliases: `ModelId`, `ConversationId`, `DownloadId`, `McpServerId`, etc. |
| `common.ts` | `Unsubscribe`, `EventHandler` |
| `models.ts` | What the model calls take: `AddModelParams`, `UpdateModelParams`, and the update body |
| `chat.ts` | `ConversationSummary`, `ChatMessage`, `CreateConversationParams` |
| `downloads.ts` | The download queue as served: `QueueSnapshot`, `DownloadRow` and its `DownloadRowText`, `FinishedDownload` (all re-exported bindings); the queue request and response; what a finished download hands the toast |
| `events.ts` | `ServerWireEvent`, `DownloadEvent`, `AppEventMap` |
| `mcp.ts` | MCP server and tool shapes |
| `verification.ts` | Model verification shapes |
| `proxy.ts` | Proxy status and configuration shapes |
| `remote.ts` | Remote tunnel shapes, all re-exported bindings — the status carries fingerprints, never a ticket |
| `dashboard.ts` | The proxy dashboard snapshot graph |
| `admission.ts` | Slot admission and residency shapes |

<!-- module-docs:end -->
