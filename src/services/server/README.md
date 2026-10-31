# server

<!-- module-docs:start -->

Safe action wrappers around server lifecycle operations. A stop of a server that is already stopped counts as done, so UI components never manage server state directly; any other failure is thrown to the caller.

## Key Files

| File | Role |
|------|------|
| `safeActions.ts` | `safeStopServer(modelId)` — wraps `getTransport().stopServer()`, treats `NOT_FOUND`, `CONFLICT` or "not running" as success, rethrows anything else |

## Flow

```
UI: onClick → safeStopServer(modelId)
       ▼
  getTransport().stopServer(modelId)
       ▼
  transport → POST /api/servers/stop
       ▼
  Success: server registry updates via event stream
  Already stopped (NOT_FOUND, CONFLICT, "not running"): resolves
  Other failure: rethrown to the caller
```

<!-- module-docs:end -->
