# hooks

<!-- module-docs:start -->

Custom hooks for the chat messages panel: message deletion, live timer ticks, and AI title generation. Loading a conversation belongs to the runtime (`hooks/useGglibRuntime/useRunReader.ts`). They are called from the panel root so that state which touches the thread runtime stays in the component that owns it. `useContextReading.ts` is the exception: it reads the thread itself, from the ring and the image tile that draw it, so the root hands nothing down for it.

## Key Files

| File | Role |
|------|------|
| `useMessageDeletion.ts` | Cascade delete with confirmation modal; reloads and resets the thread afterwards |
| `useSharedTicker.ts` | Shared 1-second tick counter running only during active streaming; consumed by `ThinkingTimingContext` |
| `useTitleGeneration.ts` | The title's rename field, and its AI title, made from the chat's saved rows by a backend LLM prompt. The head's button asks for one, after a question when it would replace a title somebody chose. The first reply to finish while the chat is open (a run the page read to its end, ended `completed`) asks for one by itself if the chat is still called `New Chat` then; opening a chat asks for none |
| `useImageUrl.ts` | A `blob:` URL for an image: of the file the page holds, or of its bytes read from the chat's store with the page's credential; revoked when it goes |
| `usePictureRedraw.ts` | One picture drawn again on this machine: the picture and the mask of what was brushed sent, the render followed, stopped or failed, and the result stored and kept beside its source only on Keep |
| `usePictureKeeping.ts` | What the open chat's drawn pictures are fixed and redone with (none on a far chat or an unsaved one), and the chat read again once a newer picture is kept |
| `useContextReading.ts` | The open thread's context reading, for the composer's ring and an image's share of the context; null when the thread gives none |

<!-- module-docs:end -->
