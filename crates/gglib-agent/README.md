# gglib-agent

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-complexity.json)

Pure-domain agentic loop implementation for gglib.

Backs `gglib chat` and `gglib q`, and enforces the defences a local model needs
to finish a tool-calling task: loop detection, stagnation detection, and context
pruning.  The loop and stagnation detectors themselves live in
`gglib_core::domain::agent` so the proxy can enforce the same guards on
`/v1/chat/completions` — this crate consumes them per iteration.

## Architecture

This crate is in the **Application Layer** — it orchestrates the LLM→tool→LLM
cycle using only injected port traits from `gglib-core`.  It has **zero
infrastructure dependencies**: no HTTP, no MCP internals, no Axum, no database.

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Overview

This crate implements:
- **`AgentLoop`** — concrete implementation of `AgentLoopPort`; drives the
  ReAct-style LLM→tool→LLM cycle until a final answer or termination condition
- **Guard enforcement** — runs `gglib_core::domain::agent`'s `LoopDetector` on
  tool-call-producing iterations, and `StagnationDetector` (repeated-response
  hashing, windowed) on iterations that made no tool call. The loop detector counts
  back-to-back repeats only, and only those that got the same answer back, so
  it needs telling twice: `check` before the batch executes and
  `record_results` once its answers exist
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
| `guards` | The stagnation and loop detectors behind one verdict |
| `stream_collector` | Consumes `LlmStreamEvent` stream, forwards text live |
| `tool_execution` | Parallel tool dispatch with semaphore + timeout |
| `util` | Shared internal utilities |
<!-- MODULE_TABLE_END -->
