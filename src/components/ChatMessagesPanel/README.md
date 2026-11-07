# ChatMessagesPanel

<!-- module-docs:start -->

Central chat interface: the thread is one reading column of at most 680 px, turns 32 px apart with no lines between them, a question a filled block and a reply plain 16 px text, with no margin; it manages the message thread, system prompt, conversation operations (rename, clear, export), and AI title generation. `ChatMessagesPanel.tsx` is a thin composition root: it owns the `@assistant-ui/react` thread runtime and the state that touches it, and delegates everything else to the named children in `components/` and the hooks in `hooks/`.

## Architecture

```
ChatMessagesPanel                 ← composition root; owns the thread runtime
    ├── MessageActionsContext     ← delete, whose chat it is, fixing and redoing a picture
    │     └── BranchingContext    ← the branch points, and the changes made from them
    │           └── ImageInputContext   ← whether the composers offer images
    │                 └── ThinkingTimingProvider  ← context for live reasoning timers
    │                       └── ThreadPrimitive   ← @assistant-ui message list, in the panel's column
    │                             ├── ChatPanelHeader     ← head row: the title ▾, rename
    │                             │     └── SystemPromptLine  (own draft state)
    │                             ├── ChatStatusBanners   ← the chat error
    │                             ├── ReadingColumn       ← the turns, at most 680 px
    │                             │     ├── RunPreviewsProvider ← the run's preview frames, for the replies
    │                             │     │     └── MessageBubbles      ← a question's filled block; a reply's plain text
    │                             │     │           ├── ReplyArrival (until the first token: phase, prompt reading, bar)
    │                             │     │           ├── ThinkingBlock (the fold) → ToolCallRow per tool call
    │                             │     │           ├── MarkdownMessageContent, MessageImages / ToolImages
    │                             │     │           └── TurnFoot: time on hover, FigureLine, TurnActions, BranchSwitcher
    │                             │     ├── BranchEnd           ← where other branches go on past the last message
    │                             │     └── Unanswered          ← the failed run's line, or Retry under a question nothing answers
    │                             └── ComposerFooter      ← tiles, input; [+] menu, chips, Thinking, ring, model ▾, Send / Stop; or ModelGoneLine
```

A far chat (`source="far"`, the machine this one is joined to) is read and carried on but not changed here: the head shows its title alone, and its turns offer Copy alone. A question names the paired device behind it when its row does ("Other machine" for one typed at the far machine), and names no one when it was sent from here.

On this machine's chats a question and a reply can each be edited, a reply retried, and any turn branched from. An edit or retry that would rewrite a saved reply is made on a new branch of the chat, which the page opens, saying the original is kept (ADR 0017); the panel holds no rule of when that happens. Where the chat's family holds other turns, the turn's foot shows `‹ 2/3 ›`, and choosing one opens its chat. Delete… asks through the one confirm sheet (`app/confirm`).

## Words this folder says that Rust does not

State words come from `components/status/words.ts`; figures and their words from `readings/turnFigures`.

- the fold: `Thought for 3.0 s · 2 tool calls`, `Thinking… 4.2 s`, `Thought`, `1 tool call`; a tool row: `on <server>`, `running`, `failed · <reason>`, Arguments, Result, Copy / Copied
- the figure line: `unfinished · 4.7 s · 117 tok/s · 282 tokens reused`; its tooltip: `<model> · <quant>`, `for <device>`, `cut off before it finished`, `thought for 19 s`, `3,180 tokens read`, `496 tokens written`
- the action line: Copy / Copied · Retry · Edit · ⋯ (Branch from here · Delete…); their names Copy reply, Copy message, Retry this reply, Edit reply, Edit message, More for this reply, More for this message
- the confirm sheet: "Delete this message?" / "The N messages after it are deleted too. This cannot be undone." / Delete; the refusal: "A reply is still being written here. Stop it or wait, then delete."
- the lines past the last turn: "Other branches go on from here.", "Nothing answers this yet." Retry, "The reply failed: <the run's reason>." Retry
- a question's sender: `<device>`, "Other machine"; a reply with nothing: "Stopped before it wrote anything.", "No text."; the arrival line: the phases of `ArrivingPhase`, `1,240 of 3,420 tokens read`, `2,100 from cache`

## Key Files

| File | Role |
|------|------|
| `ChatMessagesPanel.tsx` | Composition root; wires the thread runtime, hooks, and child components together |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | Every child of the root — panel chrome (`ChatPanelHeader`, `SystemPromptLine`, `ChatStatusBanners`, `ComposerFooter` with `PlusMenu`, `ModelPicker`, `ModelGoneLine`, `AgentLimitsSheet`) and message rendering (`MessageBubbles`, `ThinkingBlock`, `ToolCallRow`, `TurnFoot`, `TurnActions`, `ReplyArrival`, `BranchSwitcher`, `BranchingContext`, `Unanswered`, `MarkdownMessageContent`, `MessageActionsContext`) |
| `context/` | `ThinkingTimingContext` — decoupled timer updates to avoid full list re-renders; `RunPreviewsContext` — the run's preview frames, reaching a reply beside its message and never in it |
| `hooks/` | `useMessageDeletion`, `useSharedTicker`, `useTitleGeneration`, `useImageUrl`, `useContextReading`, `useModelLoadWait`, `usePictureRedraw`, `usePictureKeeping` |

<!-- module-docs:end -->
