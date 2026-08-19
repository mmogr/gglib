<!-- module-docs:start -->

# Hooks Module

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-hooks-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-hooks-complexity.json)

Custom React hooks for gglib GUI functionality.

## Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                   React Components                                  │
│                                         │                                           │
│                                         ▼                                           │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                              Custom Hooks                                   │   │
│   │                                                                             │   │
│   │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │   │
│   │  │  useModels  │  │ useServers  │  │ useSettings │  │  useTags    │         │   │
│   │  │   CRUD ops  │  │  Lifecycle  │  │   Config    │  │  Tagging    │         │   │
│   │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │   │
│   │         │                │                │                │                │   │
│   │         └────────────────┴────────────────┴────────────────┘                │   │
│   │                                   │                                         │   │
│   └───────────────────────────────────┼─────────────────────────────────────────┘   │
│                                       ▼                                             │
│                          ┌───────────────────────┐                                  │
│                          │  services/transport/  │                                  │
│                          │  (platform layer)     │                                  │
│                          └───────────────────────┘                                  │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

## Hooks

### Domain Hooks

| Hook | Description |
|------|-------------|
| [`useModels.ts`](useModels.ts) | Model CRUD operations and listing |
| [`useModelLibraryEvents.ts`](useModelLibraryEvents.ts) | Reload the library when another client changes it |
| [`useServers.ts`](useServers.ts) | Server lifecycle management (start/stop/health) |
| [`useTags.ts`](useTags.ts) | Model tagging operations |
| [`useMcpServers.ts`](useMcpServers.ts) | MCP server configuration |
| [`useSettings.ts`](useSettings.ts) | Application settings management |

### Download Hooks

| Hook | Description |
|------|-------------|
| [`useDownloadManager.ts`](useDownloadManager.ts) | Download queue operations and progress |
| [`useDownloadCompletionEffects.ts`](useDownloadCompletionEffects.ts) | Side effects on download completion |
| [`useDownloadSystemStatus.ts`](useDownloadSystemStatus.ts) | Whether the desktop download backend has finished initialising, or failed |

### System Hooks

| Hook | Description |
|------|-------------|
| [`useLlamaStatus.ts`](useLlamaStatus.ts) | llama.cpp installation status |
| [`useSystemMemory.ts`](useSystemMemory.ts) | System memory probes |
| [`useModelsDirectory.ts`](useModelsDirectory.ts) | Models directory configuration |
| [`useServerLogs.ts`](useServerLogs.ts) | Server log streaming |
| [`useToolSupportCache.ts`](useToolSupportCache.ts) | MCP tool support caching |
| [`useDaemonReachable.ts`](useDaemonReachable.ts) | Whether the daemon answers at all — the state the tray popover used to confuse with a stopped proxy |
| [`useProxyDashboard.ts`](useProxyDashboard.ts) | Subscribes to a running proxy's live dashboard stream, on the proxy's own host, port and credential |

### Utility Hooks

| Hook | Description |
|------|-------------|
| [`useDebounce.ts`](useDebounce.ts) | Debounced value updates |
| [`useClickOutside.ts`](useClickOutside.ts) | Click outside detection for dropdowns |
| [`useModelFilterOptions.ts`](useModelFilterOptions.ts) | Model filtering and sorting |
| [`useMetricHistory.ts`](useMetricHistory.ts) | Ring buffer of samples, either as-is or as the per-second rate of a cumulative counter |
| [`usePanelResize.ts`](usePanelResize.ts) | Draggable split with min/max bounds, optionally persisted to `localStorage` |
| [`useToastTimer.ts`](useToastTimer.ts) | Toast auto-dismiss countdown with pause and resume |

### Runtime Hook

| Hook | Description |
|------|-------------|
| [`useGglibRuntime/`](useGglibRuntime/) | Consolidated runtime state (models, servers, downloads) |
| [`useChatPersistence/`](useChatPersistence/) | Hydrating chat messages from the database and writing them back, for `ExternalStoreRuntime` |

## Usage

```tsx
import { useModels } from './hooks/useModels';
import { useServers } from './hooks/useServers';

function ModelList() {
  const { models, loading, error, refreshModels } = useModels();
  const { startServer, stopServer } = useServers();

  const handleStart = async (modelId: number) => {
    await startServer({ id: modelId, port: 8080 });
    await refreshModels();
  };
}
```

## Design Principles

1. **Single Responsibility** — Each hook manages one domain concept
2. **Composable** — Hooks can be combined for complex features
3. **Backend-Driven** — Hooks fetch from and sync to the Rust backend
4. **Error Handling** — All hooks expose error state for UI feedback

<!-- module-docs:end -->
