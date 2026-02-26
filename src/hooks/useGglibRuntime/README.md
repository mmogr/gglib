# useGglibRuntime — Prompt Layer Architecture

This document explains the **additive prompt injection** system used by the
chat runtime, why it replaced the old hot-swap pattern, and how to extend it.

---

## Background: why the old approach was removed

The original implementation used a `hotSwapDefaultSystemPrompt()` function
that did an **exact string comparison** against `DEFAULT_SYSTEM_PROMPT`:

```typescript
// old — brittle exact-match hot-swap
if (msg.content === DEFAULT_SYSTEM_PROMPT) {
  cloned[i] = { ...msg, content: TOOL_ENABLED_SYSTEM_PROMPT };
}
```

This had three failure modes:
1. **Any user customisation broke it** — even adding a single space meant tool
   instructions were silently dropped.
2. **User content was discarded** — even when matched, the entire system prompt
   was overwritten.
3. **Silent failure** — tools were declared in the API call but the model
   received no guidance on how to use them.

---

## The additive injection pattern

All prompt composition now goes through two functions in `promptBuilder.ts`:

```
buildSystemPrompt(base, layers)     → composed string
injectPromptLayers(messages, layers) → new message array
```

A **`PromptLayer`** is a plain object:

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Unique identifier (used for documentation; not deduplicated at runtime) |
| `content` | `string` | Prompt fragment to inject |
| `position` | `'prepend' \| 'append'` | Whether the fragment goes before or after the base prompt |
| `priority` | `number` | Ordering key — **lower number appears first** within each position group |

`buildSystemPrompt` assembles the final string as:

```
[...prepends sorted by priority, basePrompt, ...appends sorted by priority]
  .filter(s => s.trim() !== '')
  .join('\n\n')
```

Empty or whitespace-only segments are stripped before joining, so an empty
base prompt never produces leading/trailing blank blocks.

---

## Built-in layers and priority slots

| Layer constant | position | priority | Purpose |
|---|---|---|---|
| `TOOL_INSTRUCTIONS_LAYER` | `append` | **100** | Tool-usage guidance injected whenever tools are active |
| `FORMAT_REMINDER_LAYER` | `append` | **200** | Lightweight response-format nudge |
| `createWorkingMemoryLayer(…)` | `append` | **300** | Per-iteration tool-digest summary (created fresh each agentic step) |

Slots between these values are intentionally left open for future layers.

---

## Immutability contract

`injectPromptLayers` **never mutates its input**:

- It always returns a new array via `slice()`.
- System message objects are replaced with a spread copy
  (`{ ...original, content: composed }`) rather than mutating `.content`
  in-place — important because `slice()` is a shallow copy.
- The function returns a defensive clone even when `layers` is empty.

### Double-injection safety

`buildSystemPrompt` is **not** idempotent — it does not check whether a layer
is already present before appending.  Sequential injection calls (e.g. once
from `runAgenticLoop.ts` for working memory, then from `streamModelResponse.ts`
for tool/format layers) are safe **only** because the caller keeps its
`apiMessages` array pristine and passes a fresh clone into each call.

**The architectural guarantee lives in the caller, not in `promptBuilder.ts`.**

---

## How to add a new prompt layer

1. **Define a constant** in `promptBuilder.ts` (or in the module that owns the
   concept, if it is highly domain-specific):

   ```typescript
   export const MY_LAYER: PromptLayer = {
     id: 'my-feature',
     content: 'Instructions for my feature.',
     position: 'append',
     priority: 150, // between tool instructions (100) and format reminder (200)
   };
   ```

2. **Pass it to `injectPromptLayers`** at the call site where you build the
   API message array:

   ```typescript
   const layeredMessages = injectPromptLayers(apiMessages, [
     TOOL_INSTRUCTIONS_LAYER,
     MY_LAYER,
     FORMAT_REMINDER_LAYER,
   ]);
   ```

3. **Write a test** that asserts the layer's `id`, `position`, and `priority`
   are stable (see `promptBuilder.test.ts` for examples of the shape-invariant
   pattern).

---

## Module map

| File | Role |
|---|---|
| `promptBuilder.ts` | `PromptLayer` type, `buildSystemPrompt`, `injectPromptLayers`, built-in layers |
| `agentLoop.ts` | Loop detection, retries, context pruning, `DEFAULT_MAX_TOOL_ITERS` |
| `runAgenticLoop.ts` | Outer agentic loop; injects the working-memory layer each iteration |
| `streamModelResponse.ts` | Single LLM call with SSE streaming; injects tool + format layers |
| `useGglibRuntime.ts` | React hook; wires the above together |
| `src/constants/prompts.ts` | `DEFAULT_SYSTEM_PROMPT` — UI default, **not** used for string matching |
