<!-- module-docs:start -->

# Hooks Module

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
| [`usePairedModels.ts`](usePairedModels.ts) | The paired machine's models for the library and a chat on one of them: read while it is reached and the caller needs them, kept and marked stale while it is away or a read fails, cleared on disconnect or once another machine answers |
| [`useLibrarySelection.ts`](useLibrarySelection.ts) | The library's one selection, a model here or a far one, never both; a far pick tells the native menu nothing here is selected |
| [`useServers.ts`](useServers.ts) | Server lifecycle management (start/stop/health) |
| [`useChatModelFacts.ts`](useChatModelFacts.ts) | The chat page's model: tool-calling support, its format, quantisation, whether it reads images, its context, and whether it thinks |
| [`useImageInput.ts`](useImageInput.ts) | Whether the chat's composers offer images, by `canSee` (this machine's model, or the paired machine's row for it, read only for a chat on its model; always for a far chat), why not, and the context their cost is read against |
| [`useThinkingSwitch.ts`](useThinkingSwitch.ts) | The chat's Thinking switch: shown where its model thinks, by `thinks` (this machine's model, the paired machine's row for it, or a far chat's row for the model it last ran on); showing what gglib remembers of the chat until it is clicked; and saying the choice on a send, `off` or `default`, until the turn that says it is accepted, after which the choice shows only until the chat is next read and that reading is shown whatever it says; a choice no accepted turn has said ends when a reading of the chat agrees with it |
| [`useTags.ts`](useTags.ts) | Model tagging operations |
| [`useMcpServers.ts`](useMcpServers.ts) | MCP server configuration |
| [`useSettings.ts`](useSettings.ts) | Application settings management |

### Download Hooks

| Hook | Description |
|------|-------------|
| [`useDownloadManager.ts`](useDownloadManager.ts) | Holds the daemon's queue snapshot, from the queue route and the event stream, newest `revision` winning; queue, cancel and clear |
| [`useDownloadCompletionEffects.ts`](useDownloadCompletionEffects.ts) | Batches download completions into one library refresh and one toast; raises an error toast for each failure. A toast for one download is the daemon's own text for how it ended |

### System Hooks

| Hook | Description |
|------|-------------|
| [`useLlamaStatus.ts`](useLlamaStatus.ts) | Whether llama.cpp is installed and can be downloaded, from the daemon's setup-status route |
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
| [`useFileDropGuard.ts`](useFileDropGuard.ts) | Refuses a file dragged or dropped where nothing takes it, so the desktop window never opens it in place of the app |
| [`useModelFilterOptions.ts`](useModelFilterOptions.ts) | Model filtering and sorting |
| [`useMetricHistory.ts`](useMetricHistory.ts) | Ring buffer of samples, either as-is or as the per-second rate of a cumulative counter |
| [`usePanelResize.ts`](usePanelResize.ts) | Draggable split with min/max bounds, optionally persisted to `localStorage` |
| [`useToastTimer.ts`](useToastTimer.ts) | Toast auto-dismiss countdown with pause and resume |

### Runtime Hook

| Hook | Description |
|------|-------------|
| [`useGglibRuntime/`](useGglibRuntime/) | Consolidated runtime state (models, servers, downloads) |
| [`useChatPersistence/`](useChatPersistence/) | Turning a conversation's saved rows into thread messages; the daemon writes them, the page only reads |

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
