# events

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-events-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-events-complexity.json)

<!-- module-docs:start -->

Real-time event subscription layer over SSE (Server-Sent Events), the one implementation for every mode — no Tauri-event branch remains *in this layer*. Presents a unified `subscribe(eventType, handler)` interface. The SSE implementation uses a single pooled connection to avoid exhausting browser HTTP/2 connection limits.

Tauri's `listen()` is still used elsewhere for OS-level notifications that are not daemon news — menu commands, llama-install progress, download system status. Those are not product events and do not belong on this bus.

## Architecture

```
transport.subscribe('server', handler)
       ▼
Single SSE connection: GET /api/events
  Demultiplexes by event.type field
  Auto-reconnects with exponential backoff
       ▼
handler(payload)  ← validated via decoders/
```

One path, on desktop and on the web alike: the desktop WebView resolves the
daemon's base URL through `get_embedded_api_info` and then consumes the same
stream a browser tab does.

## Key Files

| File | Role |
|------|------|
| `index.ts` | Factory; returns the SSE event bus |
| `sse.ts` | Single SSE connection with reconnect, backoff, and subscriber demultiplexing |

<!-- module-docs:end -->
