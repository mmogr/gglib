# ChatMessagesPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-complexity.json)

<!-- module-docs:start -->

Central chat interface managing the message thread, system prompt, conversation operations (rename, clear, export), AI title generation, and toggling between standard chat and council (multi-agent) modes. Integrates the `@assistant-ui/react` thread runtime and delegates persistence to `useChatPersistence`.

## Architecture

```
ChatMessagesPanel
    ├── ThinkingTimingProvider    ← context for live reasoning timers
    ├── ThreadPrimitive           ← @assistant-ui message list
    │     └── MessageBubbles     ← per-message rendering
    │           ├── MarkdownMessageContent
    │           ├── ThinkingBlock  (collapsible CoT with live timer)
    │           └── ToolUsageBadge / ToolExecutionProgress
    └── ComposerPrimitive         ← message input + controls
          ├── system prompt editor
          ├── orchestrator toggle
          └── send / stop streaming
```

## Key Files

| File | Role |
|------|------|
| `ChatMessagesPanel.tsx` | Root component; conversation hydration, title generation, cascade delete |
| `ConfirmDeleteModal.tsx` | Warns about cascade deletion when removing a mid-thread message |
| `ThinkingBlock.tsx` | Collapsible reasoning section with live duration during streaming |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | `MessageBubbles`, `MarkdownMessageContent`, `MessageActionsContext` |
| `context/` | `ThinkingTimingContext` — decoupled timer updates to avoid full list re-renders |
| `hooks/` | `useChatPersistence`, `useSharedTicker`, `useTitleGeneration` |

<!-- module-docs:end -->
