# events

<!-- module-docs:start -->

Strict discriminated-union TypeScript types for the agent events emitted by the Rust backend over SSE. Mirrors `gglib_core::domain::agent::AgentEvent`, enabling exhaustive `switch` statements and preventing silent contract drift.

Download progress types live in `src/services/transport/types/`, not here.

## Key Files

| File | Role |
|------|------|
| `agentEvent.ts` | `AgentEvent` union: text/reasoning deltas, tool lifecycle and progress, waiting, loop control, prompt progress, per-turn usage and system warnings; and the payload of a run's `preview` side event, which is not one of them |

## AgentEvent Union Members

| Type | Trigger |
|------|---------|
| `text_delta` | Streamed text token from LLM |
| `reasoning_delta` | Chain-of-thought token (reasoning models) |
| `tool_call_start` | LLM emitted a tool call |
| `tool_call_complete` | Tool executor returned a result, with the images the tool stored, by id and facts |
| `tool_progress` | How far a long-running tool has got: its stage, pass and steps, or its place in line |
| `waiting` | The turn waits for an image render or a model load, with that work's step and its place in line |
| `iteration_complete` | One agentic loop iteration finished |
| `final_answer` | Agent produced final response |
| `error` | Unrecoverable error in the agent loop |
| `system_warning` | A non-fatal condition the loop recovered from or is recovering from; more events follow |
| `prompt_progress` | Prompt pre-fill progress: tokens processed of the total, how many were cached, and the time taken |
| `turn_usage` | How one model turn was made, once its stream ended: the model, token counts and timings |

<!-- module-docs:end -->
