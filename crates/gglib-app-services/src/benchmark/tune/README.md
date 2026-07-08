# tune

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-tune-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-tune-complexity.json)

<!-- module-docs:start -->

Tune-mode benchmark service — sweeps a model's sampling parameters against
an agentic tool-calling task suite to find the settings that make the model
both accurate at tool calls and resistant to loop/stagnation guard triggers.

# Module Layout

```text
tune/
  mod.rs      — run_tune() orchestration entrypoint (called from
                BenchmarkOps::run_tune)
  executor.rs — ScoringToolExecutorPort: a local ToolExecutorPort that
                records calls instead of executing them for real
  scoring.rs  — AST-style (BFCL-inspired) diffing of recorded calls against
                a task's expected outcome
  pruning.rs  — successive-halving candidate reduction math
```

# Why No MCP Dependency

Unlike the production agent loop, tune evaluation never talks to a real
MCP server: [`ScoringToolExecutorPort`] advertises exactly the tools a
[`TuneTask`](gglib_core::domain::benchmark::tune::task::TuneTask) declares
and returns deterministic synthetic results, so `compose_agent_loop()`
(which hardwires an MCP-backed executor) is not reusable here — this
module calls `AgentLoop::build()` directly instead.

# No Model Reload Per Candidate

Only ONE `ModelRuntimePort::ensure_model_running()` call happens per tune
run. Every candidate varies only the per-request `InferenceConfig` passed
to a fresh `LlmCompletionAdapter::with_sampling(..)` — sampling parameters
are per-request, not part of the loaded llama-server process, so a sweep
across dozens of candidates never triggers a costly model reload.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`executor.rs`](executor.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-executor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-executor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-executor-coverage.json) |
| [`pruning.rs`](pruning.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-pruning-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-pruning-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-pruning-coverage.json) |
| [`scoring.rs`](scoring.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-scoring-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-scoring-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-scoring-coverage.json) |
<!-- module-table:end -->

</details>
