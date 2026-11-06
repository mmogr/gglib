<!-- module-docs:start -->

# Pages Module

Top-level page components for the gglib GUI application.

## Architecture

Two entry points build two windows; the main one routes by address (`app/router`):

```text
main.tsx ──► App.tsx ──► router                tray-main.tsx ──► TrayPanel.tsx
                │  /library /models/:id /far/:id   (separate "tray" Tauri window:
                │  /chat/:modelId  /chat/far/:id    proxy status, endpoint, metrics)
                │  /diagnostics/benchmarks
                ▼
  places/library · ChatPage · BenchmarkPage
              ▼
        components/
   (Shared UI Components)
```

## Pages

| Page | Description |
|------|-------------|
| [`ChatPage.tsx`](ChatPage.tsx) | Chat interface for interacting with running models, at `/chat/:modelId` and `/chat/far/:id` through `app/ChatRoute` and `app/FarChatRoute` |
| [`BenchmarkPage.tsx`](BenchmarkPage.tsx) | Benchmark workflows: compare, perf, and sampling-parameter tune, at `/diagnostics/benchmarks` through `app/BenchmarkRoute` |
| [`TrayPanel.tsx`](TrayPanel.tsx) | Proxy tray window — is the endpoint up, and what is it doing (status, endpoint copy bar, connections, slots) |

`chatTabs.tsx` is a supporting piece of `ChatPage`, not a routed page: the view tabs, and the controls the notebook's head carries in its margin. So is `useChatConversations.ts`: the conversation list, from this machine or, on a computer joined with `gglib remote join`, the far machine, read live through this machine's daemon and never copied; this machine's list holds only the chats the session's machine can carry on, since a chat's machine is fixed. And `useConversationActions.ts`: delete, rename, restart, export and the system prompt, which act on this machine's daemon and so do nothing while a far chat is open.

The main window's landing page, the library at `/library`, `/models/:id` and `/far/:id`, is a place (`src/places/library/`), not a page.

### Chat Page

Interactive chat interface featuring:
- Real-time streaming responses
- Conversation history
- Model selection from the composer's margin: a server running here, or a registered model, started first
- A context ring in the composer's margin: how much of the model's context the conversation has used, from the figures its last finished reply carried, and absent where they are not known
- A Thinking switch in the composer's margin, only where the chat's model thinks: it shows what gglib remembers of the conversation, so a chat switched off here opens switched off on a paired device and the reverse, and a click is said by the next message sent
- MCP tool integration

## Design Principles

1. **Page as Composition Root** — Pages compose hooks and components
2. **Minimal Logic** — Business logic lives in hooks, not pages
3. **Responsive Layout** — Pages adapt to different screen sizes

<!-- module-docs:end -->
