# events

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-types-events-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-types-events-complexity.json)

<!-- module-docs:start -->

Strict discriminated-union TypeScript types for all events emitted by the Rust backend over SSE. Mirrors `gglib_core::domain::agent::AgentEvent` and download progress domain structs, enabling exhaustive `switch` statements and preventing silent contract drift.

On download progress events, `speed_bps` and `eta_seconds` are **optional** and
omitted by the backend when unknown, so the corresponding fields are `number | undefined`.
Absent means the rate estimator has not warmed up; it does not mean zero.

## Key Files

| File | Role |
|------|------|
| `agentEvent.ts` | `AgentEvent` union: text/reasoning deltas, tool lifecycle, loop control, cost monitoring |
| `download.ts` | `NormalizedDownloadProgress` — unified shape normalizing single-file and multi-shard events |
| `index.ts` | Barrel export |

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

## Download Normalisation

`normalizeDownloadProgress()` and `normalizeShardProgress()` collapse the backend's two event shapes (single-file vs multi-shard) into one `NormalizedDownloadProgress` interface, giving the UI a single rendering path.

<!-- module-docs:end -->
