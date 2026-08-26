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
  usage.rs    — TaskUsageTally: per-task completion-token count that survives
                a guard-aborted run
```

# What The Axes Actually Measure

Shared with the raw-vs-gglib A/B eval, which scores its two arms through
the same `axis_scores` and `compute_composite_score`. Both are easy to
misread, and both have already produced a wrong headline once.

**Tool accuracy** and **task completion** are fractions of every task in
the set — always measured, always comparable.

**Loop avoidance** is not. It is a fraction of the *loop-eligible* tasks
only: those the guards aborted, or that completed at least
`MIN_ITERATIONS_FOR_LOOP_RISK` tool-executing iterations. A run that
answered after a single tool batch never gave `LoopDetector` two
signatures to compare, so it could not have looped, and counting it as
having avoided one turns the axis into a reward for not iterating.

The bound is a lower one and always has been: two iterations make a repeat
*representable*, not inevitable, so a task whose batches all differed was
already counted as eligible while being incapable of tripping. Now that
`LoopDetector` counts consecutively, a task that alternated between two
batches is in that same position, so the denominator is slightly wider than
before. Left as is — narrowing it means replaying each task's batch sequence
inside the scorer, which is a heavier instrument than the axis is worth. When
nothing was eligible the axis is `None` — unmeasured, not perfect — and
`compute_composite_score` drops it and renormalises the remaining weight
rather than imputing a score.

Note that `iterations` counts *tool-executing* turns: the agent loop emits
`IterationComplete` only after executing a turn's calls, so a turn that
answered in text is not counted and a guard-aborted run reports one fewer
than the turn it aborted on.

**Nothing here measures cost.** Two arms can be identical on all three
axes while one spends three orders of magnitude more time and tokens
reaching them. That is what `ArmScores`' efficiency figures — suite wall
time, suite completion tokens, mean time to first tool call — are for, and
why they are reported beside the composite rather than folded into it: a
blended score would stop being comparable across machines.

# Why No MCP Dependency

Unlike the production agent loop, tune evaluation never talks to a real
MCP server: [`ScoringToolExecutorPort`] advertises exactly the tools a
[`TuneTask`](gglib_core::domain::benchmark::tune::task::TuneTask) declares
and returns deterministic synthetic results, so `compose_agent_loop()`
(which hardwires an MCP-backed executor) is not reusable here — this
module calls `AgentLoop::build()` directly instead.

# No Model Reload Per Candidate

Only ONE `ModelRuntimePort::admit()` call happens per tune
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
| [`apply_run.rs`](apply_run.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-apply_run-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-apply_run-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-apply_run-coverage.json) |
| [`executor.rs`](executor.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-executor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-executor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-executor-coverage.json) |
| [`pruning.rs`](pruning.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-pruning-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-pruning-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-pruning-coverage.json) |
| [`scoring.rs`](scoring.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-scoring-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-scoring-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-scoring-coverage.json) |
| [`usage.rs`](usage.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-usage-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-usage-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tune-usage-coverage.json) |
<!-- module-table:end -->

</details>
