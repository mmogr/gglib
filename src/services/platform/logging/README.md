# logging

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-platform-logging-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-services-platform-logging-complexity.json)

<!-- module-docs:start -->

Unified, strictly-typed application logger with per-category filtering, frontend-side log-level gating, payload truncation, and dual-transport output (browser console + Tauri tracing IPC). All log categories are a TypeScript union type — misspellings fail at compile time.

## Data Flow

```
appLogger.info('service.download', 'Download started', { modelId })
          ▼
  Category check (TS union — compile-time safety)
          ▼
  Level check: isLevelEnabled(level, VITE_LOG_LEVEL)
          ▼
  truncatePayload(payload)   ← prevents oversized IPC messages
          ▼
  MultiTransport routes to:
    ├── ConsoleTransport      → console.log/warn/error
    └── TauriTracingTransport → invoke('plugin:log|...')
```

## Key Files

| File | Role |
|------|------|
| `appLogger.ts` | Singleton logger; category-typed `debug/info/warn/error` methods |
| `types.ts` | `LogLevel`, `LogEntry`, `ILogger` interface; `isLevelEnabled()`, `parseLogLevel()` |
| `transports.ts` | `ConsoleTransport`, `TauriTracingTransport`, `MultiTransport` |
| `truncate.ts` | Caps payload size to avoid flooding the Tauri IPC channel |

## Configuration

Set `VITE_LOG_LEVEL=debug|info|warn|error` in `.env.local`. The logger filters on the frontend before any IPC call, keeping the Tauri channel clean in production builds.

<!-- module-docs:end -->
