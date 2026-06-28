# events

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-events-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-events-complexity.json)

<!-- module-docs:start -->

Real-time event subscription layer supporting both Tauri IPC events and web SSE (Server-Sent Events). Presents a unified `subscribe(eventType, handler)` interface regardless of platform. The SSE implementation uses a single pooled connection to avoid exhausting browser HTTP/2 connection limits.

## Architecture

```
transport.subscribe('server', handler)
       ▼
Platform detection
       │
       ├── Tauri mode:
       │     Subscribes to granular event names
       │       (server:started, server:stopped, server:error, …)
       │     All route to the same handler
       │
       └── Web mode:
             Single SSE connection: GET /api/events
             Demultiplexes by event.type field
             Auto-reconnects with exponential backoff
       ▼
handler(payload)  ← validated via decoders/
```

## Key Files

| File | Role |
|------|------|
| `index.ts` | Factory; returns `TauriEventBus` or `SseEventBus` based on platform |
| `tauri.ts` | Subscribes to multiple Tauri event names; fans out to subscribers |
| `sse.ts` | Single SSE connection with reconnect, backoff, and subscriber demultiplexing |
| `eventNames.ts` | Constants mapping logical event types to backend event-name strings |

<!-- module-docs:end -->
