# components

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-components-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-components-complexity.json)

<!-- module-docs:start -->

Rendering sub-components for the chat message thread: per-message bubble layout, rich Markdown content with syntax highlighting, and the context that wires message-level action buttons (edit, copy, delete) to their handlers.

## Key Files

| File | Role |
|------|------|
| `MessageBubbles.tsx` | User/assistant/system message containers; thinking blocks, tool badges, action buttons |
| `MarkdownMessageContent.tsx` | Parses and renders message text as Markdown (remark-gfm + rehype-highlight) |
| `MessageActionsContext.tsx` | React context providing edit/copy/delete callbacks to nested message components |

<!-- module-docs:end -->
