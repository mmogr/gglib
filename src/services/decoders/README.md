# decoders

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-decoders-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-decoders-complexity.json)

<!-- module-docs:start -->

Type-safe runtime decoders for backend event payloads arriving over SSE streams. Converts raw JSON into typed TypeScript objects and validates against known event schemas. Failures throw in development (fast fail on contract drift) and degrade gracefully with a warning in production.

## Data Flow

```
SSE raw JSON payload
        ▼
decodeDownloadEvent(raw)
   ├── Is object?          → throw / warn
   ├── Known event type?   → throw / warn
   ├── Required fields?    → throw / warn
   └── Cast to typed union
        ▼
DownloadEvent  (typed, safe to use in UI)
```

## Key Files

| File | Role |
|------|------|
| `downloadEvent.ts` | Validates and decodes SSE download event payloads against the `DownloadEventType` union |

Decoders act as the I/O boundary guard — if the Rust backend renames an event type, the decoder fails fast in development instead of silently producing `undefined` in production.

<!-- module-docs:end -->
