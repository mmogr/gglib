# components

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-components-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ChatMessagesPanel-components-complexity.json)

<!-- module-docs:start -->

Every child of the `ChatMessagesPanel` composition root: the panel chrome around the thread (header, system prompt card, banners, composer, delete modal) and the per-message rendering (the notebook's rows of margin and body, Markdown with syntax highlighting, collapsible reasoning, and the context wiring message-level actions to their handlers).

## Panel chrome

| File | Role |
|------|------|
| `ChatPanelHeader.tsx` | The notebook's head row: title or rename field, AI title generation, conversation actions; the page's controls in its margin |
| `SystemPromptSection.tsx` | System prompt card; owns its own draft/edit state and reports only on save |
| `ChatStatusBanners.tsx` | Chat error banner and the read-only warning shown while the server is down |
| `ComposerFooter.tsx` | The composer on the turns' grid: model, quantisation and tools in the margin; input and Stop/Send in the body |
| `ModelPicker.tsx` | The composer margin's model: on this machine a picker of the servers running here and the models that are not, handing the choice to the page, and locked while the page's session says a model is starting; beside it, Unload, which stops the model for every proxy client and leaves the chat open; plain text for a chat with another machine |
| `ConfirmDeleteModal.tsx` | Warns about cascade deletion when removing a mid-thread message |

## Message rendering

| File | Role |
|------|------|
| `MessageBubbles.tsx` | User/assistant/system turns as notebook rows; the reply's reasoning and tool calls behind "How this was made"; action buttons |
| `TurnRow.tsx` | One notebook row: margin (who, then how it was made) and body; the margin moves above the body in a narrow notebook |
| `TurnMargin.tsx` | The margin's content: who and when, a reply's figures, a reply arriving and how far its prompt was read |
| `turnFigures.ts` | Which figures a turn's margin shows: only those the page has for that turn (model, quantisation, thought, tool calls, tokens read and cached, time and a computed rate), never a zero or a dash for one it lacks |
| `MarkdownMessageContent.tsx` | Parses and renders message text as Markdown (remark-gfm + rehype-highlight) |
| `ThinkingBlock.tsx` | Collapsible reasoning section with live duration during streaming |
| `MessageActionsContext.tsx` | React context providing edit/copy/delete callbacks to nested message components |

<!-- module-docs:end -->
