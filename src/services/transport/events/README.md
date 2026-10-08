# events

<!-- module-docs:start -->

Real-time event subscription layer over SSE (Server-Sent Events), the one implementation for every mode — no Tauri-event branch remains *in this layer*. Presents a unified `subscribe(eventType, handler)` interface, and `onEventStreamOpen(handler)`, which fires each time the connection opens, reconnections included. The SSE implementation uses a single pooled connection to avoid exhausting the browser's HTTP/1.1 per-origin connection limit (6 slots).

Tauri's `listen()` is still used elsewhere for OS-level notifications that are not daemon news — menu commands. Those are not product events and do not belong on this bus.

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

The stream has no backlog: what was sent while the connection was down is
gone, and the daemon behind a new connection may be a new process. A listener
that holds state built from events reads that state again on
`onEventStreamOpen`. `useDownloadManager` does, and forgets the last queue
`revision` it took, since a restarted daemon numbers its snapshots from 1.

## Key Files

| File | Role |
|------|------|
| `sse.ts` | Single SSE connection with reconnect and subscriber demultiplexing; its `subscribeSseEvent` and `onEventStreamOpen` are the event bus `getTransport()` hands out |
| `backoff.ts` | The wait before a reconnection: doubling, capped, with jitter |
| `open.ts` | The signal that the stream opened, for listeners that re-read state |
| `category.ts` | Wire-tag → category routing; a tag with no arm here is dropped silently |

<!-- module-docs:end -->
