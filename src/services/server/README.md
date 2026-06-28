# server

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-server-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-server-complexity.json)

<!-- module-docs:start -->

Safe action wrappers around server lifecycle operations. Encapsulates error handling and toast notification triggering so that UI components never manage server state directly or deal with unhandled rejections.

## Key Files

| File | Role |
|------|------|
| `safeActions.ts` | `safeStopServer(modelId)` — wraps `clients/servers.stopServer()`, catches errors, triggers toast on failure |

## Flow

```
UI: onClick → safeStopServer(modelId)
       ▼
  Error boundary
       ▼
  clients/servers.stopServer(modelId)
       ▼
  transport → POST /api/servers/stop
       ▼
  Success: server registry updates via event stream
  Failure: toast notification shown, no unhandled rejection
```

<!-- module-docs:end -->
