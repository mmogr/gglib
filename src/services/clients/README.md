# clients

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-clients-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-clients-complexity.json)

<!-- module-docs:start -->

Clients that own real request logic of their own. Each one exists because it does something `getTransport()` cannot: parse a streaming response by hand, or talk to a server that is not the app's own backend. Everything else — plain request/response against the backend — belongs on the transport interface and is called directly as `getTransport().method()`. There is no facade layer in between.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                 React Components                    │
└───────────────┬─────────────────────┬───────────────┘
                │                     │
                │                     ▼
                │        ┌─────────────────────────┐
                │        │  clients/               │
                │        │  streaming + off-app    │
                │        │  endpoints only         │
                │        └────────────┬────────────┘
                ▼                     │
┌─────────────────────────────────────▼───────────────┐
│              transport/  (Platform Layer)           │
│         Tauri IPC  ──or──  HTTP + SSE               │
└─────────────────────────────────────────────────────┘
```

`council.ts` and `benchmark.ts` reach the backend through `transport/api/client`'s authenticated fetch helpers. `proxyDashboard.ts` bypasses the transport entirely — a running proxy serves its dashboard stream on its own port.

## Key Files

| File | Role |
|------|------|
| `council.ts` | Council (multi-agent orchestrator) runs — manual SSE parser over a chunked `TextDecoder` buffer |
| `benchmark.ts` | Benchmark and tune runs — REST endpoints plus an SSE progress stream |
| `proxyDashboard.ts` | Live proxy dashboard — native `EventSource` against the running proxy's own HTTP port, not the app backend |

## Contract

A module belongs here only if it needs streaming or a non-backend origin. If a new operation is a plain call against the app's backend, add it to the `Transport` interface and call `getTransport()` from the consumer — do not add a wrapper module here.

No client may import from `platform/` — platform exceptions are handled inside `transport/platform/`.

<!-- module-docs:end -->
