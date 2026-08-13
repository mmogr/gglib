# ChatMessagesPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-complexity.json)

<!-- module-docs:start -->

Central chat interface managing the message thread, system prompt, conversation operations (rename, clear, export), and AI title generation. `ChatMessagesPanel.tsx` is a thin composition root: it owns the `@assistant-ui/react` thread runtime and the state that touches it, and delegates everything else to the named children in `components/` and the hooks in `hooks/`.

## Architecture

```
ChatMessagesPanel                 ← composition root; owns the thread runtime
    ├── ChatPanelHeader           ← title, rename, AI title, status, tool support
    ├── SystemPromptSection       ← prompt preview + editor (own draft state)
    ├── ChatStatusBanners         ← chat error / server-down warning
    ├── ThinkingTimingProvider    ← context for live reasoning timers
    │     └── ThreadPrimitive     ← @assistant-ui message list
    │           ├── MessageBubbles      ← per-message rendering
    │           │     ├── MarkdownMessageContent
    │           │     ├── ThinkingBlock  (collapsible CoT with live timer)
    │           │     └── ToolUsageBadge / ToolExecutionProgress
    │           └── ComposerFooter      ← input, send / stop
    └── ConfirmDeleteModal        ← cascade-delete confirmation
```

## Key Files

| File | Role |
|------|------|
| `ChatMessagesPanel.tsx` | Composition root; wires the thread runtime, hooks, and child components together |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | Every child of the root — panel chrome (`ChatPanelHeader`, `SystemPromptSection`, `ChatStatusBanners`, `ComposerFooter`, `ConfirmDeleteModal`) and message rendering (`MessageBubbles`, `MarkdownMessageContent`, `ThinkingBlock`, `MessageActionsContext`) |
| `context/` | `ThinkingTimingContext` — decoupled timer updates to avoid full list re-renders |
| `hooks/` | `useThreadHydration`, `useMessageDeletion`, `useSharedTicker`, `useTitleGeneration`, `buildThreadMessages` |

<!-- module-docs:end -->
