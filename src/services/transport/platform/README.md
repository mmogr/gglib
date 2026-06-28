# platform

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-platform-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-transport-platform-complexity.json)

<!-- module-docs:start -->

Transport-layer platform operations (llama installation status, URL opening) with separate Tauri and web implementations. Exposes these capabilities through the standard `Transport` interface so they are accessible via `getTransport().platform.*` rather than bypassing the transport boundary.

## Key Files

| File | Role |
|------|------|
| `index.ts` | Factory; returns `TauriPlatform` or `WebPlatform` |
| `tauri.ts` | `checkLlamaStatus()`, `installLlama()`, `openUrl()` via Tauri `invoke()` |
| `web.ts` | `checkLlamaStatus()` / `installLlama()` throw `NOT_SUPPORTED`; `openUrl()` uses `window.open` |

## Platform Branching

```
getPlatformTransport()
       ▼
  isTauri()?
  ├── Yes → TauriPlatform (real native operations)
  └── No  → WebPlatform  (browser stubs or NOT_SUPPORTED)
```

Contrast with `services/platform/` — that module is for shell integration imported directly by UI components (a transport exception). This module routes through the transport interface.

<!-- module-docs:end -->
