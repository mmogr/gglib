# components

<!-- module-docs:start -->

Every child of the `ChatMessagesPanel` composition root: the panel chrome around the thread (header, system prompt line, the error banner, the composer and its `[+]` menu, delete modal) and the per-message rendering (the notebook's rows of margin and body, Markdown with syntax highlighting, collapsible reasoning, and the context wiring message-level actions to their handlers).

## Panel chrome

| File | Role |
|------|------|
| `ChatPanelHeader.tsx` | The chat's head: the title (16/600) as the menu button for Rename · Generate title · System prompt… · Export · Start over (same prompt)… — Delete…, or the rename field; a far chat's title is plain |
| `SystemPromptLine.tsx` | Nothing for the default prompt, "Custom system prompt · Edit" for the chat's own, and the inline editor (Reset to default, Cancel, Save); owns its draft and reports only a save |
| `ChatStatusBanners.tsx` | The chat error banner. A model unloaded or crashed mid-chat has none, and no toast: the composer is replaced by `ModelGoneLine` |
| `ComposerFooter.tsx` | The composer: the images attached as tiles above the text, the text box ("Ask <model> anything" in a new chat, "Reply…" in a saved one once its turns are there, and nothing while they load; the bare model name, never the picker's "<model> on desk"), then one row of `[+]`, the chips that are set (the folder, `Draw ×`), the Thinking toggle (a switch labelled Thinking, neutral either way, drawn only where `useThinkingSwitch` says the model thinks), the context ring, the model, and Send (ink, ↑) which becomes Stop while a reply is written. The model is the picker on this machine's chat; plain text on the paired machine's ("<model> on desk", "desk picks the model"), and, while that machine is away (from the remote registry alone, CH-9), "desk is away since 14:02. Your message will wait." with Send disabled; "Loading <model>… 12 s" ("Loading the model… 12 s" on a far chat) while the newest reply waits for its model to load (`useModelLoadWait`). Where this machine's model is gone, the whole composer is `ModelGoneLine`. Paste and drop take images only where `ImageInputContext` says the model reads them |
| `ModelGoneLine.tsx` | What stands in for the composer while this machine's model is gone: "<model> was unloaded. [Load it again]" (the picker's switch, back onto the same model; "Loading <model>…" while it starts) or "<model> stopped unexpectedly · Show log". The read-only lock, until H2 (`feat(gui)/chat-without-lock`) removes it |
| `PlusMenu.tsx` | The composer's `[+]`: Add image…, Use a folder…, Draw a picture (▸ where there is an image model to choose) and Tools ▸, from `composerMenus`; the tool registry is read again each time it opens, so a tool registered since, built-in or MCP, is listed without opening anything else (CH-11); Tools ▸ ends with Agent limits…, which opens `AgentLimitsSheet` |
| `composerMenus.ts` | The `[+]` menu's items, pure: Add image… greyed with the model's reason (or "The model is not loaded."), Use a folder… none for a far chat and greyed for a model that calls no tools, Draw a picture greyed with the machine's reason or a submenu of the default rule and each image model (one lacking a file greyed, its title what it lacks; a choice no longer listed kept, so it can be taken back), Tools ▸ the checklist (Enable all, Disable all, one box per tool) or "This model can't call tools" |
| `AgentLimitsSheet.tsx` | The agent limits (`components/AgentLimitsSection`) in a small sheet, asking once, the first time it opens, whether the chat's model's template reads a reasoning effort. An interim home until Settings › Tools › Agent limits takes it (S1 `feat(gui)/settings-place`, S2 `feat(gui)/settings-tools`) |
| `FolderChip.tsx` | The chat's folder (`useChatFolder`), drawn only once one is set: its name, the whole path on hover, a click to choose another, × to clear it; a warning, its title the reason, where a send was refused for it. Choosing opens the native dialog on desktop and a typed path field on web (Enter uses it, Escape leaves it; no form of its own, since it sits in the composer's, and neither key sends the drafted message), which `[+]` › Use a folder… opens too |
| `ContextRing.tsx` | The composer's ring `◔` and its detail: drawn only when the conversation has a reading (`readings/contextReading`), "Context: 2,140 of 32,768 used" on hover, the detail on a click, and from 70% the figure beside it in the warning colour, from 90% the danger colour |
| `ComposerImages.tsx` | `ImageInputContext`, which the panel gives the thread so the page's composer and every edit's read one answer (whether the model reads images, why not, its context; nothing offered without it); a composer's images as tiles above its text (thumbnail, "uploading…" or "not uploaded", "~N tokens · P% of context" on hover, × to remove), the cost against the context the ring reads and, before the conversation has a reading, against the one `ImageInputContext` gives; and `useAddImage`, the file picker Add image… opens |
| `imageCost.ts` | An image's cost as its tile says it: "~N tokens", and "P% of context" when a context size is known |
| `ModelPicker.tsx` | The composer's model ▾ on this machine's chat: Loaded (the chat's own ticked), On this Mac, and the paired machine's models while one is connected (in a new chat, each opens a new chat on that model; in a saved chat each is listed greyed, its reason "A chat stays on its machine…" said under it), then Unload <model> (every endpoint client loses it; the chat stays open) and Show <model> (its inspector, where the Log is); the choice is handed to the page, and while a model starts the picker is "Loading <model>…"; plain text for a chat with another machine |
| `ConfirmDeleteModal.tsx` | Warns about cascade deletion when removing a mid-thread message |

## Message rendering

| File | Role |
|------|------|
| `MessageBubbles.tsx` | User/assistant/system turns as notebook rows; the reply's reasoning and tool calls behind "How this was made" (a tool still running with how far it has got and the picture it is making, from `RunPreviewsContext`), and the images its tools made under its text, outside that fold and always shown; action buttons, Edit on a question and on a reply with text, and Branch from here on each; the edit composer, Send for a question and Save for a reply, with a question's images, each removable, and a paste into it taking images only where the model reads them |
| `Unanswered.tsx` | The row under a chat that ends in a question nothing answers, with Retry, which answers it; not while a reply is being read |
| `BranchingContext.tsx` | What the open chat offers of its branches (its branch points, whether it ends in a question, Retry, Branch from here, and opening another chat), for the turns and rows that show it; the same object until the chat's thread is read again, so no turn draws again for a render of the page; none on a far chat |
| `BranchSwitcher.tsx` | A branch point's options: previous and next, and the count that lists each option by its line ("Nothing here yet" for a branch with nothing there), the open chat's marked; in the margin of the turn that starts the point (`TurnBranches`), and in a row after the last message where other branches go on (`BranchEnd`) |
| `MessageImages.tsx` | A user turn's images, and the strip of the images a reply's tools made: each read from the chat's store by id with the page's credential as a `blob:` URL, shown small and enlarged in a dialog on a click. A picture the drawing tool made has Fix or redo… in that dialog, where the chat can keep the result (`MessageActionsContext.pictures`) |
| `PictureEditor.tsx` | Fixing or redoing a drawn picture: Fix brushes a mask over the part to draw again, Redo draws the whole picture again from itself; the draw's prompt to edit, a strength slider, an image model where more than one can draw, the render's stage and frame with Stop, then the result with hold-to-compare, Try again, Discard and Keep, which alone adds it to the chat |
| `TurnRow.tsx` | One notebook row: margin (who, then how it was made) and body; the margin moves above the body in a narrow notebook |
| `TurnMargin.tsx` | The margin's content: who and when, a reply's figures, a reply arriving and how far its prompt was read, or what it waits for and the step a render in its way is on |
| `MarkdownMessageContent.tsx` | Parses and renders message text as Markdown (remark-gfm + rehype-highlight) |
| `ThinkingBlock.tsx` | Collapsible reasoning section with live duration during streaming |
| `MessageActionsContext.tsx` | React context providing edit/copy/delete callbacks to nested message components |

## Words

The composer's words that are its own, not the daemon's (state words, Loading and Loaded, Away, come from `components/status/words.ts`):

- `[+]` (named "Add to the message"): Add image…, Use a folder…, Draw a picture, Tools, Agent limits…; "This model can't call tools", "No tools registered", "The model is not loaded.", Enable all, Disable all, "Default (<model>)", "Model <id> (no longer listed)", "Missing: <roles>", and the titles "Let the next message draw a picture with <model>. It applies to that message only.", "This model calls tools (<format> format)."
- chips: `Draw ×` ("Don't draw"), the folder's name × ("Clear the folder"), "Folder path"
- Thinking (its two titles), Send, Stop, "Ask <model> anything", "Reply…"
- the ring: "Context: 2,140 of 32,768 used"
- the picker: On this Mac, "On <machine>", Unload <model>, Show <model>, "A chat stays on its machine. Start a new chat to use this model.", "Opens a new chat on <model>, on <machine>."
- the lines: "<model> was unloaded.", Load it again, "<model> stopped unexpectedly", Show log, "<machine> is away since 14:02. Your message will wait."
- tiles: "uploading…", "not uploaded"; the agent limits sheet: "Agent limits", "Apply to every chat this device sends."

<!-- module-docs:end -->
