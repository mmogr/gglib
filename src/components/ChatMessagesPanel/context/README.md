# context

<!-- module-docs:start -->

React context for decoupled timing updates during reasoning stream. Provides a shared tick counter to `ThinkingBlock` components so they can update live duration displays without triggering a re-render of the entire message list. A second context carries the preview frames of the run being read to the reply that shows them.

## Key Files

| File | Role |
|------|------|
| `ThinkingTimingContext.tsx` | Context + provider wrapping the message list; exposes tick value incremented by `useSharedTicker` |
| `RunPreviewsContext.tsx` | The frames a tool of the run being read is making, by tool call; they reach a reply this way and never through its message, so nothing that keeps a message keeps a frame |

Keeping the ticker in a dedicated context means only `ThinkingBlock` components subscribe — not the full `MessageBubbles` tree.

<!-- module-docs:end -->
