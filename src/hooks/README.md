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
│   │  │ useServers  │  │ useSettings │  │useMcpServers│  │ useProxyKey │         │   │
│   │  │Registry view│  │   Config    │  │ MCP servers │  │  Reveal     │         │   │
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
| [`usePairedModels.ts`](usePairedModels.ts) | The paired machine's models for the library and a chat on one of them: read while it is reached and the caller needs them, kept and marked stale while it is away or a read fails, cleared on disconnect or once another machine answers |
| [`useServers.ts`](useServers.ts) | Running servers from the event-driven registry, and stopping one |
| [`useChatModelFacts.ts`](useChatModelFacts.ts) | The chat page's model: tool-calling support, its format, quantisation, whether it reads images, its context, and whether it thinks |
| [`useImageInput.ts`](useImageInput.ts) | Whether the chat's composers offer images, by `canSee` (this machine's model, or the paired machine's row for it, read only for a chat on its model; always for a far chat), why not, and the context their cost is read against |
| [`useThinkingSwitch.ts`](useThinkingSwitch.ts) | The chat's Thinking switch: shown where its model thinks, by `thinks` (this machine's model, the paired machine's row for it, or a far chat's row for the id its settings record, never a name); showing what gglib remembers of the chat until it is clicked; and saying the choice on a send, `off` or `default`, until the turn that says it is accepted, after which the choice shows only until the chat is next read and that reading is shown whatever it says; a choice no accepted turn has said ends when a reading of the chat agrees with it |
| [`useChatFolder.ts`](useChatFolder.ts) | The chat's folder, where gglib's own file tools read in its runs on this machine ([ADR 0023](../../docs/adr/0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md)): the choice made here (picked with the native dialog, or typed on web, or cleared), else what gglib remembers (`settings.folder`); said on the run that changes it, the path or `""` for Clear, until that turn is accepted or a reading agrees; an error state after a run refused for its folder (`folder_unusable`, `folder_missing`) until another is chosen, it is cleared or a turn is accepted; none on a far chat, and none offered to a model that calls no tools unless its chat has one; held in memory per chat, moved to the chat a first send makes |
| [`useChatTools.ts`](useChatTools.ts) | The chat's tools, as the composer's Tools ▸ ticks them: what the chat saved (`settings.tools`, by the daemon's names, an entry with no `:` naming any tool of that bare name; none named is every tool, `no_tools` none), written with only the settings a change changes as it is made (`PUT /api/conversations/{id}`), the last untick saving `no_tools` since an empty list is every tool; shown as chosen until the write lands and the chat is read again, the reading again after a refused one with a toast saying why; a chat not yet made holds its choice until its first send saves it before its turn starts (`forCreate`); none on a far chat |
| [`useDrawSwitch.ts`](useDrawSwitch.ts) | The composer's Draw button: greyed, with the reason, until the machine that would draw answers that a message can (`drawingAvailability`: this machine's answer, told that the model is the paired machine's or calls no tools, or the far machine's for a far chat) and whenever it answers that it cannot; pressed, armed for the open chat's next start, which says `draw`: a send, or an answer run (an edit, a regenerate or a Retry) started while it is armed; no longer armed once that turn is accepted, and still armed after a refused one. It also holds the open chat's choice of image model, where that machine lists two or more: a pick is asked about (`image_model` on the question, so one that cannot draw greys the button with the machine's reason), presses the button, and is said as `imageModel` on every start that says `draw` while it stands; held in memory per chat and per machine, moved to the chat a first turn makes, and never sent to a machine whose answer listed no models |
| [`useMcpServers.ts`](useMcpServers.ts) | MCP server configuration |
| [`useSettings.ts`](useSettings.ts) | Application settings management |

### Download Hooks

| Hook | Description |
|------|-------------|
| [`useDownloadToasts.ts`](useDownloadToasts.ts) | How downloads ended, as toasts on any screen, mounted once by the app's layout: completions batched by `services/downloadsEvents` (one download in the daemon's words, several counted), each failure its own error toast. The queue itself is `services/downloadsRegistry` |

### System Hooks

| Hook | Description |
|------|-------------|
| [`useEngine.ts`](useEngine.ts) | One engine's status, its install (kept across remounts, one per engine) and its removal, for the engine cards in Settings › Engines; `useInstalledVersion` for the inspector's drawing parts |
| [`useLlamaStatus.ts`](useLlamaStatus.ts) | Whether llama.cpp is installed and can be downloaded, and the update to offer (a model running on it or not; the daemon refuses an install under one), from the daemon's setup-status route |
| [`useModelsDirectory.ts`](useModelsDirectory.ts) | Models directory configuration |
| [`useServerLogs.ts`](useServerLogs.ts) | Server log streaming |
| [`useToolSupportCache.ts`](useToolSupportCache.ts) | Whether a HuggingFace model supports tool calling, cached in memory and shared across components |
| [`useDaemonReachable.ts`](useDaemonReachable.ts) | Whether the daemon answers at all, by its `/health` (`probeDaemon`: no token, no key, no prompt) — the state the tray popover used to confuse with a stopped proxy |
| [`useDaemonRecovery.ts`](useDaemonRecovery.ts) | `useDaemonReachable` with the registries kept to it: the proxy, remote and server state let go while the daemon is gone, and on its return the event stream woken and each read again; the tray popover and the window's `ServiceGate` |
| [`useProxyKey.ts`](useProxyKey.ts) | The proxy's API key, revealed each time a surface that presents it opens (ADR 0027): the dashboard stream, the tray panel's stream and Clear Cache |

### Utility Hooks

| Hook | Description |
|------|-------------|
| [`useClickOutside.ts`](useClickOutside.ts) | Click outside detection for dropdowns |
| [`useFileDropGuard.ts`](useFileDropGuard.ts) | Refuses a file dragged or dropped where nothing takes it, so the desktop window never opens it in place of the app |
| [`useToastTimer.ts`](useToastTimer.ts) | Toast auto-dismiss countdown with pause and resume |

### Runtime Hook

| Hook | Description |
|------|-------------|
| [`useGglibRuntime/`](useGglibRuntime/) | The assistant-ui chat adapter: starts agent runs on the daemon and turns their events into thread messages |
| [`useChatPersistence/`](useChatPersistence/) | Turning a conversation's saved rows into thread messages; the daemon writes them, the page only reads |

## Usage

```tsx
import { useServers } from './hooks/useServers';
import { useModelLibrary } from './services/modelRegistry';
import { refreshModels } from './services/modelEvents';

function ModelList() {
  const { models, loading, error } = useModelLibrary();
  const { servers, stopServer } = useServers();

  const handleStop = async (modelId: number) => {
    await stopServer(modelId);
    await refreshModels();
  };
}
```

## Design Principles

1. **Single Responsibility** — Each hook manages one domain concept
2. **Composable** — Hooks can be combined for complex features
3. **Backend-Driven** — Hooks fetch from and sync to the Rust backend
4. **Error Handling** — Hooks that load data expose error state for UI feedback

<!-- module-docs:end -->
