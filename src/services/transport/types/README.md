# types

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-types-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-types-complexity.json)

<!-- module-docs:start -->

All TypeScript types, interfaces, and DTOs forming the contract between the frontend and backend. Defines the composed `Transport` interface (which extends all domain sub-interfaces), branded ID types, common utility types, and per-domain request/response shapes.

## Interface Hierarchy

```
Transport
    extends ModelsTransport       (models.ts)
    extends ChatTransport         (chat.ts)
    extends ServersTransport      (servers.ts)
    extends DownloadsTransport    (downloads.ts)
    extends EventsTransport       (events.ts)
    extends McpTransport          (mcp.ts)
    extends SettingsTransport     (settings.ts)
    extends ProxyTransport        (proxy.ts)
    extends TagsTransport         (tags.ts)
    extends BuiltinTransport      (builtin.ts)
    extends VerificationTransport (verification.ts)
```

## Key Files

| File | Role |
|------|------|
| `index.ts` | Composes all domain sub-interfaces into the unified `Transport` type |
| `ids.ts` | Brand-tagged ID types: `ModelId`, `ConversationId`, `DownloadId`, `McpServerId`, etc. |
| `common.ts` | `Unsubscribe`, `EventHandler`, base error types |
| `models.ts` | Model shapes and `ModelsTransport` interface |
| `chat.ts` | `ConversationSummary`, `ChatMessage`, `SaveMessageParams`, `ChatTransport` |
| `servers.ts` | `ServeConfig`, `ServerInfo`, `ServeResponse`, `ServersTransport` |
| `downloads.ts` | Download queue types and `DownloadsTransport` |
| `events.ts` | `ServerEvent`, `DownloadEvent`, `ServerStateInfo`, `EventsTransport` |

## Branded ID Types

IDs are branded (`ModelId`, `ConversationId`, etc.) to prevent accidental cross-domain ID substitution at compile time.

<!-- module-docs:end -->
