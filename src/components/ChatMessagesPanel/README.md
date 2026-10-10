# ChatMessagesPanel

<!-- module-docs:start -->

Central chat interface, laid out as a notebook (each turn a row: a margin saying who and how it was made, then the body), managing the message thread, system prompt, conversation operations (rename, clear, export), and AI title generation. `ChatMessagesPanel.tsx` is a thin composition root: it owns the `@assistant-ui/react` thread runtime and the state that touches it, and delegates everything else to the named children in `components/` and the hooks in `hooks/`.

## Architecture

```
ChatMessagesPanel                 ← composition root; owns the thread runtime
    ├── ThinkingTimingProvider    ← context for live reasoning timers
    │     └── ThreadPrimitive     ← @assistant-ui message list, one notebook column
    │           ├── ChatPanelHeader     ← head row: title, rename, AI title, actions
    │           │     └── SystemPromptSection  (own draft state)
    │           ├── ChatStatusBanners   ← chat error / server-down warning
    │           ├── MessageBubbles      ← each turn a TurnRow: margin + body
    │           │     ├── TurnMargin (who; figures or arrival, from turnFigures)
    │           │     ├── BranchSwitcher (the options where the chat's branches part)
    │           │     ├── MarkdownMessageContent
    │           │     └── ThinkingBlock / ToolUsageBadge / ToolExecutionProgress
    │           ├── BranchEnd           ← where other branches go on past the last message
    │           ├── Unanswered          ← Retry, under a chat that ends in a question
    │           └── ComposerFooter      ← model, tools, context ring, Thinking switch and Draw button; input, send / stop
    └── ConfirmDeleteModal        ← cascade-delete confirmation
```

In the page's Console view the panel draws only its head (`headOnly`), and the page puts the server beneath it; the thread and composer stay mounted, hidden.

A far chat (`source="far"`, the machine this one is joined to) is read and carried on but not changed here: the head shows its title alone, and no turn is edited, regenerated or deleted. The margin names the paired device behind a turn when its row does, and says "You" only for this machine's own user turns.

On this machine's chats a question and a reply can each be edited, a reply regenerated, and any turn branched from. An edit or regenerate that would rewrite a saved reply is made on a new branch of the chat, which the page opens, saying the original is kept (ADR 0017); the panel holds no rule of when that happens. Where the chat's family holds other turns, the turn's margin shows the options, and choosing one opens its chat.

## Key Files

| File | Role |
|------|------|
| `ChatMessagesPanel.tsx` | Composition root; wires the thread runtime, hooks, and child components together |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | Every child of the root — panel chrome (`ChatPanelHeader`, `SystemPromptSection`, `ChatStatusBanners`, `ComposerFooter`, `ConfirmDeleteModal`) and message rendering (`MessageBubbles`, `BranchSwitcher`, `BranchingContext`, `Unanswered`, `MarkdownMessageContent`, `ThinkingBlock`, `MessageActionsContext`) |
| `context/` | `ThinkingTimingContext` — decoupled timer updates to avoid full list re-renders |
| `hooks/` | `useMessageDeletion`, `useSharedTicker`, `useTitleGeneration`, `useImageUrl`, `useContextReading` |

<!-- module-docs:end -->
