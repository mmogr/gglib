# messages

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-utils-messages-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-utils-messages-complexity.json)

<!-- module-docs:start -->

Message conversion and serialization utilities — the single source of truth for translating between the `@assistant-ui/react` runtime's `ThreadMessage`, database persistence rows, and markdown transcripts. Ensures identical transformation logic everywhere, preventing inconsistencies between display, storage, and export.

## Key Files

| File | Role |
|------|------|
| `contentParts.ts` | `reconstructContent()` rebuilds a saved row's tool calls for the thread; `extractReasoningText()` joins a message's reasoning parts |
| `turnMade.ts` | How a reply's turn was made (model, token counts, times), from a run's `turn_usage` event or a saved row's metadata, in one shape |
| `threadMessageToTranscriptMarkdown.ts` | Converts a `ThreadMessage` to plain-text Markdown (answer only, no reasoning, no tool calls) |

## Persistence Contract

| Content Type | Storage Location |
|-------------|-----------------|
| Text content | `messages.content` column (Markdown) |
| Reasoning / CoT | `messages.metadata.thinking` |
| Tool-calls | `messages.metadata.contentParts[]` |
| Images | `messages.images[]`: each by id, with its type and size and without its bytes |
| `argsText` | Computed at persistence boundary, never stored in React state |

## Transcript Extraction

`threadMessageToTranscriptMarkdown()` intentionally excludes reasoning tokens and tool-call parts — the transcript reflects only the user-visible answer text, which is the correct representation for export, title generation, and search indexing.

<!-- module-docs:end -->
