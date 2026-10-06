# decoders

<!-- module-docs:start -->

The runtime check on download events arriving over the SSE stream. It looks at the event's `type` tag and nothing else: a payload that is not an object, has no string `type`, or has a `type` that is not one of the five known is logged as an error and dropped. It does not throw, and it does not check the fields beside the tag.

## Data Flow

```
SSE raw JSON payload
        ▼
decodeDownloadEvent(raw)
   ├── Is object?          → log, return null
   ├── Has a string type?  → log, return null
   ├── Known event type?   → log, return null
   └── Cast to typed union
        ▼
DownloadEvent  (typed by its tag; its fields are trusted)
```

## Key Files

| File | Role |
|------|------|
| `downloadEvent.ts` | Checks a download event's `type` against the five the daemon sends |

The five are `queue_snapshot`, `download_completed`, `download_failed`, `download_cancelled` and `queue_run_complete`. `queue_snapshot` carries the whole queue, the same `QueueSnapshot` the REST route serves; the other four say that something ended. The list is a `Record` keyed by the generated `DownloadEvent['type']`, so a variant added or removed in Rust is a compile error here until the list follows.

<!-- module-docs:end -->
