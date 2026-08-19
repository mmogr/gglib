# events

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-types-events-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-types-events-complexity.json)

<!-- module-docs:start -->

Strict discriminated-union TypeScript types for the agent events emitted by the Rust backend over SSE. Mirrors `gglib_core::domain::agent::AgentEvent`, enabling exhaustive `switch` statements and preventing silent contract drift.

Download progress types live in `src/services/transport/types/`, not here.

## Key Files

| File | Role |
|------|------|
| `agentEvent.ts` | `AgentEvent` union: text/reasoning deltas, tool lifecycle, loop control, cost monitoring |

## AgentEvent Union Members

| Type | Trigger |
|------|---------|
| `text_delta` | Streamed text token from LLM |
| `reasoning_delta` | Chain-of-thought token (reasoning models) |
| `tool_call_start` | LLM emitted a tool call |
| `tool_call_complete` | Tool executor returned a result |
| `iteration_complete` | One agentic loop iteration finished |
| `final_answer` | Agent produced final response |
| `error` | Unrecoverable error in the agent loop |
| `prompt_progress` | Token count and timing metadata |

<!-- module-docs:end -->
