# events

<!-- module-docs:start -->

Real-time event subscription layer over SSE (Server-Sent Events), the one implementation for every mode — no Tauri-event branch remains *in this layer*. Presents a unified `subscribe(eventType, handler)` interface, `onEventStreamOpen(handler)`, which fires each time the connection opens, reconnections included, and `onEventStreamClose(handler)` and `isEventStreamOpen()`, for each end of an open connection and whether one is open now. The SSE implementation uses a single pooled connection to avoid exhausting the browser's HTTP/1.1 per-origin connection limit (6 slots).

Tauri's `listen()` is still used elsewhere for OS-level notifications that are not daemon news — menu commands. Those are not product events and do not belong on this bus.

## Architecture

```
transport.subscribe('server', handler)
       ▼
Single SSE connection: GET /api/events
  Demultiplexes by event.type field
  Auto-reconnects with exponential backoff
       ▼
handler(payload)  ← download frames validated via decoders/;
                    every other frame passed through as sent
```

One path, on desktop and on the web alike: the desktop WebView resolves the
daemon's base URL through `get_embedded_api_info` and then consumes the same
stream a browser tab does.

The stream has no backlog: what was sent while the connection was down is
gone, and the daemon behind a new connection may be a new process. A listener
that holds state built from events reads that state again on
`onEventStreamOpen`; every store does, through `services/bridgeEvents`'
`keptCurrent`, and `services/downloadsEvents` forgets the last queue
`revision` it took first, since a restarted daemon numbers its snapshots from 1.
The opening is also the signal that the daemon answers: a store whose first
read it refused (a cold start) is filled then, with no retry of its own. While
the stream is open a read that fails is asked again by `keptCurrent`, and the
end of an open stream (`onEventStreamClose`) stops that until it opens again.

While the daemon is gone the reconnect wait doubles up to 30 s. A reader that
knows the daemon is back (the tray's poll) calls `reconnectEventStreamNow()`
to connect at once rather than sleep out that wait; asked while an attempt
is out (its refusal still being handled included), the wake is kept, and if
that attempt fails the next is made at once; asked while connected (the
probe failed but the stream stayed up), the opening is announced again on
the same connection, so the stores read again. A refusal for a stale token
(a restarted desktop service minted a new one) is the daemon answering:
once the token is renewed the next attempt is made at once, and only one
such in a row. Each start of the loop owns its state, abort controller and
all, so a loop stopped while it slept ends when it wakes, and a stopped
loop still settling its last attempt cannot change the state of the loop
that replaced it (a StrictMode remount): one stream.

## Key Files

| File | Role |
|------|------|
| `sse.ts` | Single SSE connection with reconnect and subscriber demultiplexing; its `subscribeSseEvent`, `onEventStreamOpen`, `onEventStreamClose`, `isEventStreamOpen` and `reconnectEventStreamNow` are the event bus `getTransport()` hands out |
| `backoff.ts` | The wait before a reconnection: doubling, capped, with jitter |
| `open.ts` | The signal that the stream opened, for listeners that re-read state |
| `category.ts` | Wire-tag → category routing; a tag with no arm here is dropped silently |

<!-- module-docs:end -->
