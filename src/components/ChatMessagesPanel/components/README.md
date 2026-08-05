# components

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-components-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-components-complexity.json)

<!-- module-docs:start -->

Every child of the `ChatMessagesPanel` composition root: the panel chrome around the thread (header, system prompt card, banners, composer, delete modal) and the per-message rendering (bubble layout, Markdown with syntax highlighting, collapsible reasoning, and the context wiring message-level actions to their handlers).

## Panel chrome

| File | Role |
|------|------|
| `ChatPanelHeader.tsx` | Title bar: rename field, AI title generation, status badge, tool-support indicator, conversation actions |
| `SystemPromptSection.tsx` | System prompt card; owns its own draft/edit state and reports only on save |
| `ChatStatusBanners.tsx` | Chat error banner and the read-only warning shown while the server is down |
| `ComposerFooter.tsx` | Composer input, stop/send controls, thinking indicator |
| `ConfirmDeleteModal.tsx` | Warns about cascade deletion when removing a mid-thread message |

## Message rendering

| File | Role |
|------|------|
| `MessageBubbles.tsx` | User/assistant/system message containers; thinking blocks, tool badges, action buttons |
| `MarkdownMessageContent.tsx` | Parses and renders message text as Markdown (remark-gfm + rehype-highlight) |
| `ThinkingBlock.tsx` | Collapsible reasoning section with live duration during streaming |
| `MessageActionsContext.tsx` | React context providing edit/copy/delete callbacks to nested message components |

<!-- module-docs:end -->
