# components

<!-- module-docs:start -->

Every child of the `ChatMessagesPanel` composition root: the panel chrome around the thread (header, system prompt card, banners, composer, delete modal) and the per-message rendering (the notebook's rows of margin and body, Markdown with syntax highlighting, collapsible reasoning, and the context wiring message-level actions to their handlers).

## Panel chrome

| File | Role |
|------|------|
| `ChatPanelHeader.tsx` | The notebook's head row: title or rename field, AI title generation, conversation actions; the page's controls in its margin |
| `SystemPromptSection.tsx` | System prompt card; owns its own draft/edit state and reports only on save |
| `ChatStatusBanners.tsx` | Chat error banner and the read-only warning shown while the server is down |
| `ComposerFooter.tsx` | The composer on the turns' grid: model, quantisation, tools, the context ring and the Thinking switch (a button named Thinking, pressed while the chat thinks and reading "Thinking off" beside another icon while it does not, drawn only where `useThinkingSwitch` says the model does) in the margin; in the body the attached images, then attach, input and Stop/Send, with paste and drop taking images only where `ImageInputContext` says the model reads them |
| `ContextRing.tsx` | The composer margin's context ring and its detail: drawn only when the conversation has a reading, the sentence on hover, the detail on a click, and from 70% a warning icon and the figure beside it |
| `contextReading.ts` | The conversation's context reading, from the figures of the reply that decides it (the newest, passing over an unfinished one without counts), and the sentences its detail says; nothing when a figure is missing. The rule of `contracts/context/readings.json` |
| `ComposerImages.tsx` | `ImageInputContext`, which the panel gives the thread so the page's composer and every edit's read one answer (whether the model reads images, why not, its context; nothing offered without it); a composer's images as tiles (thumbnail, uploading or not uploaded or what it costs, Remove), the cost against the context the ring reads and, before the conversation has a reading, against the one `ImageInputContext` gives; and the attach button, disabled with the reason where the model cannot read images |
| `imageCost.ts` | An image's cost as its tile says it: "~N tokens", and "P% of context" when a context size is known |
| `ModelPicker.tsx` | The composer margin's model: on this machine a picker of the servers running here and the models that are not, handing the choice to the page, and locked while the page's session says a model is starting; beside it, Unload, which stops the model for every proxy client and leaves the chat open; plain text for a chat with another machine |
| `ConfirmDeleteModal.tsx` | Warns about cascade deletion when removing a mid-thread message |

## Message rendering

| File | Role |
|------|------|
| `MessageBubbles.tsx` | User/assistant/system turns as notebook rows; the reply's reasoning and tool calls behind "How this was made"; action buttons; an edit's images, each removable, and a paste into an edit taking images only where the model reads them |
| `MessageImages.tsx` | A user turn's images: read from the chat's store with the page's credential as `blob:` URLs, shown small and enlarged in a dialog on a click |
| `TurnRow.tsx` | One notebook row: margin (who, then how it was made) and body; the margin moves above the body in a narrow notebook |
| `TurnMargin.tsx` | The margin's content: who and when, a reply's figures, a reply arriving and how far its prompt was read |
| `turnFigures.ts` | Which figures a turn's margin shows: only those the page has for that turn (model, quantisation, thought, tool calls, tokens read and cached, time and a computed rate), never a zero or a dash for one it lacks |
| `MarkdownMessageContent.tsx` | Parses and renders message text as Markdown (remark-gfm + rehype-highlight) |
| `ThinkingBlock.tsx` | Collapsible reasoning section with live duration during streaming |
| `MessageActionsContext.tsx` | React context providing edit/copy/delete callbacks to nested message components |

<!-- module-docs:end -->
