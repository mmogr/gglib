# useGglibRuntime

React hook that drives the chat runtime by delegating the agentic loop to the
Rust backend (`POST /api/agent/chat`) and streaming the results back to the UI.

---

## Architecture

```
useGglibRuntime
  └── streamAgentChat()      POST /api/agent/chat  → SSE AgentEvent stream
        ├── text_delta       → append text to current assistant message
        ├── thinking         → append reasoning part
        ├── tool_call_start  → add pending tool-call part
        ├── tool_call_complete → stamp result onto tool-call part
        ├── iteration_complete → finalize current message, open next
        ├── final_answer     → finalize last message, done
        └── error            → surface error text, done
```

All loop orchestration (context pruning, tool execution, stagnation detection,
loop detection) lives in the Rust `gglib-agent` crate.

---

## Module map

| File | Role |
|---|---|
| `useGglibRuntime.ts` | React hook; wires user input → `streamAgentChat` → message state |
| `streamAgentChat.ts` | Backend SSE consumer; converts UI messages → wire format, processes events |
| `agentMessageState.ts` | Pure state-mutation helpers for in-flight assistant messages |
| `agentSseReader.ts` | Minimal POST-capable SSE reader (async generator) |
| `wireMessages.ts` | `GglibMessage[]` → backend wire-format conversion |
| `reasoningTiming.ts` | Tracks per-message reasoning segment durations |
| `clock.ts` | Monotonic clock abstraction for timing |
| `index.ts` | Public barrel export |

---

## Message-per-iteration model

One React `GglibMessage` (role `assistant`) is created for each backend
iteration.  Tool-calling iterations open a new message at `iteration_complete`;
the final-answer iteration closes the last message at `final_answer`.  This
preserves the multi-message UI layout from the previous client-side loop.

---

## Configuration

`useGglibRuntime` accepts optional overrides forwarded to the backend:

| Option | Backend field | Default |
|---|---|---|
| `maxToolIterations` | `AgentConfig::max_iterations` | 25 |
| `supportsToolCalls` | `tool_filter: []` when `false` | all tools |

Internal tuning parameters (`max_stagnation_steps`, `context_budget_chars`,
etc.) are controlled by the backend's `AgentConfig::default()` and are not
exposed to untrusted callers.

When `supportsToolCalls === false`, an empty `tool_filter` is sent so the
backend exposes no tools to the model.
