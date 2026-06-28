# context

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-context-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-context-complexity.json)

<!-- module-docs:start -->

React context for decoupled timing updates during reasoning stream. Provides a shared tick counter to `ThinkingBlock` components so they can update live duration displays without triggering a re-render of the entire message list.

## Key Files

| File | Role |
|------|------|
| `ThinkingTimingContext.tsx` | Context + provider wrapping the message list; exposes tick value incremented by `useSharedTicker` |

Keeping the ticker in a dedicated context means only `ThinkingBlock` components subscribe — not the full `MessageBubbles` tree.

<!-- module-docs:end -->
