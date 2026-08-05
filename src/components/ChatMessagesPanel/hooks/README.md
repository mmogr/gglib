# hooks

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-hooks-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-hooks-complexity.json)

<!-- module-docs:start -->

Custom hooks for the chat messages panel: thread hydration, message deletion, live timer ticks, and AI title generation. They are called from the panel root so that state which touches the thread runtime stays in the component that owns it.

## Key Files

| File | Role |
|------|------|
| `useChatPersistence.ts` | Hydrates the thread from the DB when the conversation changes; maintains the position -> DB ID map |
| `buildThreadMessages.ts` | Turns DB rows into runtime messages — shared by hydration and the post-delete reload so both fold tool rows identically |
| `useMessageDeletion.ts` | Cascade delete with confirmation modal; reloads and resets the thread afterwards |
| `useSharedTicker.ts` | Shared 1-second tick counter running only during active streaming; consumed by `ThinkingTimingContext` |
| `useTitleGeneration.ts` | Generates conversation titles from the first user message via a backend LLM prompt |

<!-- module-docs:end -->
