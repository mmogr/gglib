# gglib-agent

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-complexity.json)

Pure-domain agentic loop implementation for gglib.

## Architecture

This crate is in the **Application Layer** — it orchestrates the LLM→tool→LLM
cycle using only injected port traits from `gglib-core`.  It has **zero
infrastructure dependencies**: no HTTP, no MCP internals, no Axum, no database.

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Overview

This crate implements:
- **`AgentLoop`** — concrete implementation of `AgentLoopPort`; drives the
  ReAct-style LLM→tool→LLM cycle until a final answer or termination condition
- **Loop detection** — FNV-1a batch-signature tracking ported from the TypeScript
  frontend (`agentLoop.ts`)
- **Stagnation detection** — catches models that repeat the same response
- **Parallel tool execution** — bounded concurrency with per-tool timeout
- **Stream collection** — consumes `LlmCompletionPort` stream, forwards text
  deltas in real-time, accumulates tool-call deltas until `Done`
- **Context pruning** — drops old tool messages when the conversation exceeds the
  configured character budget

## Dependency Graph

```text
gglib-agent
    └── gglib-core (domain types + port traits only)
```

`gglib-agent` does **not** depend on `gglib-mcp`, `gglib-axum`, `reqwest`, or
any other infrastructure crate.  Concrete `LlmCompletionPort` and
`ToolExecutorPort` implementations are injected at the composition root.

## Internal Structure

<!-- MODULE_TABLE_START -->
| Module | Responsibility |
|--------|----------------|
| `agent_loop` | `AgentLoop` struct + `AgentLoopPort` impl (main state machine) |
| `context_pruning` | Budget-aware message trimming |
| `fnv1a` | FNV-1a hash primitive used by loop detection |
| `loop_detection` | Batch-signature tracking, `LoopDetector` guard |
| `stagnation` | Text-hash stagnation detection, `StagnationDetector` |
| `stream_collector` | Consumes `LlmStreamEvent` stream, forwards text live |
| `tool_execution` | Parallel tool dispatch with semaphore + timeout |
| `util` | Shared internal utilities |
| `council/config` | `CouncilConfig`, `CouncilAgent`, `SuggestedCouncil` |
| `council/events` | `CouncilEvent` SSE enum (wire format) |
| `council/prompts` | Prompt templates + contentiousness mapping |
| `council/state` | Round/contribution accumulator |
| `council/history` | Per-turn context builder (identity + transcript + directed rebuttals) |
| `council/stream_bridge` | `AgentEvent` → `CouncilEvent` mapper |
| `council/round` | Sequential round execution (per-agent turn driver) |
| `council/synthesis` | Synthesis pass (transcript → unified answer) |
| `council/judge` | Post-round judge evaluation + adaptive early stopping |
| `council/orchestrator` | Slim coordinator (rounds → judge → synthesis) |
| `council/suggest` | `suggest_council()` — shared suggest orchestration |
<!-- MODULE_TABLE_END -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`agent_loop.rs`](src/agent_loop.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-agent_loop-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-agent_loop-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-agent_loop-coverage.json) |
| [`fnv1a.rs`](src/fnv1a.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-fnv1a-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-fnv1a-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-fnv1a-coverage.json) |
| [`stream_collector.rs`](src/stream_collector.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-stream_collector-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-stream_collector-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-stream_collector-coverage.json) |
| [`util.rs`](src/util.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-util-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-util-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-util-coverage.json) |
| [`context_pruning/`](src/context_pruning/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-context_pruning-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-context_pruning-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-context_pruning-coverage.json) |
| [`council/`](src/council/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-coverage.json) |
| [`loop_detection/`](src/loop_detection/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-coverage.json) |
| [`stagnation/`](src/stagnation/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-stagnation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-stagnation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-stagnation-coverage.json) |
| [`tool_execution/`](src/tool_execution/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tool_execution-coverage.json) |
<!-- module-table:end -->

</details>
