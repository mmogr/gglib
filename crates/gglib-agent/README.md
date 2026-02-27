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
| `loop_detection` | FNV-1a hash, batch signature, `LoopDetector` guard |
| `stagnation` | Text-hash stagnation detection, `StagnationDetector` |
| `tool_execution` | Parallel tool dispatch with semaphore + timeout |
| `stream_collector` | Consumes `LlmStreamEvent` stream, forwards text live |
| `context_pruning` | Budget-aware message trimming |
<!-- MODULE_TABLE_END -->
