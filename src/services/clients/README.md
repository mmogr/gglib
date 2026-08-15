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
│              HTTP  +  SSE                           │
└─────────────────────────────────────────────────────┘
```

`benchmark.ts` reaches the backend through `transport/api/client`'s authenticated fetch helpers. `proxyDashboard.ts` bypasses the transport entirely — a running proxy serves its dashboard stream on its own port, and carries that proxy's own credential (the `proxyApiKey` setting, passed in by callers) rather than the backend session's.

## Key Files

| File | Role |
|------|------|
| `benchmark.ts` | Benchmark and tune runs — REST endpoints plus an SSE progress stream |
| `proxyDashboard.ts` | Live proxy dashboard — fetch-based SSE (`utils/sse`) against the running proxy's own HTTP port, not the app backend. Not `EventSource`: it cannot send the `Authorization` header the proxy requires |

## Contract

A module belongs here only if it needs streaming or a non-backend origin. If a new operation is a plain call against the app's backend, add it to the `Transport` interface and call `getTransport()` from the consumer — do not add a wrapper module here.

No client may import a *transport* from `platform/` — what platform difference remains is absorbed inside `transport/api/client.ts`. `proxyDashboard.ts` imports `appLogger` from there, which is a log sink rather than a way of reaching the backend.

<!-- module-docs:end -->
