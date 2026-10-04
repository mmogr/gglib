# hooks

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-hooks-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-hooks-complexity.json)

<!-- module-docs:start -->

Custom hooks for the chat messages panel: message deletion, live timer ticks, and AI title generation. Loading a conversation belongs to the runtime (`hooks/useGglibRuntime/useRunReader.ts`). They are called from the panel root so that state which touches the thread runtime stays in the component that owns it.

## Key Files

| File | Role |
|------|------|
| `useMessageDeletion.ts` | Cascade delete with confirmation modal; reloads and resets the thread afterwards |
| `useSharedTicker.ts` | Shared 1-second tick counter running only during active streaming; consumed by `ThinkingTimingContext` |
| `useTitleGeneration.ts` | Generates conversation titles from the first user message via a backend LLM prompt |
| `useImageUrl.ts` | A `blob:` URL for an image: of the file the page holds, or of its bytes read from the chat's store with the page's credential; revoked when it goes |

<!-- module-docs:end -->
