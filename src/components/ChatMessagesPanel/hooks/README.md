# hooks

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-hooks-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-hooks-complexity.json)

<!-- module-docs:start -->

Custom hooks for the chat messages panel: message persistence, live timer ticks, and AI title generation.

## Key Files

| File | Role |
|------|------|
| `useChatPersistence.ts` | Hydrates messages from DB; persists new/changed messages with debounce and deduplication |
| `useSharedTicker.ts` | Shared 1-second tick counter running only during active streaming; consumed by `ThinkingTimingContext` |
| `useTitleGeneration.ts` | Generates conversation titles from the first user message via a backend LLM prompt |

<!-- module-docs:end -->
