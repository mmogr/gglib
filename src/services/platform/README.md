# platform

<!-- module-docs:start -->

OS-specific utilities that cannot be cleanly abstracted through the transport layer: shell integration (URL opening, native file dialogs), menu bar state synchronization, and the unified application logger. Modules here are intentionally marked `TRANSPORT_EXCEPTION` — they touch OS APIs directly rather than routing through the standard transport interface.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│              UI Components (React)                   │
│          (may import from platform/ directly)        │
└─────────────────────┬────────────────────────────────┘
                      ▼
┌──────────────────────────────────────────────────────┐
│              platform/  (Shell Integration)          │
│                                                      │
│   detect.ts ─────── isDesktop()                     │
│        │                                            │
│        ├── Tauri ──► native invoke()                │
│        └── Web  ──► browser API or no-op            │
│                                                      │
│   logging/  ─────── Unified appLogger               │
│                     ConsoleTransport + TauriTransport│
└──────────────────────────────────────────────────────┘
```

## Key Files

| File | Role |
|------|------|
| `detect.ts` | `isDesktop()` — platform detection |
| `openUrl.ts` | Opens URLs in system browser (Tauri: native; Web: `window.open`) |
| `fileDialogs.ts` | Native GGUF file picker (Tauri only) |
| `menuSync.ts` | Synchronises native menu bar item state with application state |
| `menuEvents.ts` | Listens for native menu click events |
| `serverLogs.ts` | Fetches and streams llama-server log output |
| `logging/` | Strictly typed logger with categories, levels, and multi-target transports |

## Transport Exception Policy

Components needing shell integration import from `platform/` directly. All other backend communication must go through `clients/` → `transport/`. `clients/` and `transport/` never reach the backend through `platform/`: what they import from here is the logger and, in `transport/api/client.ts` alone, `isDesktop()`.

<!-- module-docs:end -->
