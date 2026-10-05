<!-- module-docs:start -->

# Pages Module

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-complexity.json)

Top-level page components for the gglib GUI application.

## Architecture

Two entry points build two windows from this directory:

```text
main.tsx ──► App.tsx                        tray-main.tsx ──► TrayPanel.tsx
                │                           (separate "tray" Tauri window:
                ▼                            proxy status, endpoint, metrics)
  ┌────────────────────────┐
  │ ModelControlCenterPage │──swaps to──► ChatPage / BenchmarkPage
  └───────────┬────────────┘
              ▼
        components/
   (Shared UI Components)
```

## Pages

| Page | Description |
|------|-------------|
| [`ModelControlCenterPage.tsx`](ModelControlCenterPage.tsx) | Model catalog and control: library, server control, and downloads — the main window's landing page |
| [`ChatPage.tsx`](ChatPage.tsx) | Chat interface for interacting with running models |
| [`BenchmarkPage.tsx`](BenchmarkPage.tsx) | Benchmark workflows: compare, perf, and sampling-parameter tune |
| [`TrayPanel.tsx`](TrayPanel.tsx) | Proxy tray window — is the endpoint up, and what is it doing (status, endpoint copy bar, connections, slots) |

`chatTabs.tsx` is a supporting piece of `ChatPage`, not a routed page: the view tabs, and the controls the notebook's head carries in its margin. So is `useChatConversations.ts`: the conversation list, from this machine or, on a computer joined with `gglib remote join`, the far machine, read live through this machine's daemon and never copied; this machine's list holds only the chats the session's machine can carry on, since a chat's machine is fixed. And `useConversationActions.ts`: delete, rename, restart, export and the system prompt, which act on this machine's daemon and so do nothing while a far chat is open.

### Model Control Center

The main window's landing page containing:
- Model list with metadata display
- Server start/stop controls and proxy control (with the Proxy Dashboard modal)
- Download queue management
- MCP server configuration
- Settings management

### Chat Page

Interactive chat interface featuring:
- Real-time streaming responses
- Conversation history
- Model selection from the composer's margin: a server running here, or a registered model, started first
- A context ring in the composer's margin: how much of the model's context the conversation has used, from the figures its last finished reply carried, and absent where they are not known
- A Thinking switch in the composer's margin, only where the chat's model thinks: it shows what gglib remembers of the conversation, so a chat switched off here opens switched off on a paired device and the reverse, and a click is said by the next message sent
- MCP tool integration

## Sub-modules

| Directory | Description |
|-----------|-------------|
| [`modelControlCenter/`](modelControlCenter/) | Components specific to the Model Control Center page |

## Design Principles

1. **Page as Composition Root** — Pages compose hooks and components
2. **Minimal Logic** — Business logic lives in hooks, not pages
3. **Responsive Layout** — Pages adapt to different screen sizes

<!-- module-docs:end -->
