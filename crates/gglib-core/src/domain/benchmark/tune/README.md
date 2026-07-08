# tune

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-benchmark-tune-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-benchmark-tune-complexity.json)

<!-- module-docs:start -->

Inference-tuning domain types: sweep a model's sampling parameters
(temperature, top-p, top-k, min-p, repeat-penalty) against an agentic
tool-calling task suite to find the settings that make the model both
accurate at tool calls and resistant to loop/stagnation guard triggers.

# Modules

| Module | Contents |
|--------|----------|
| [`config`] | [`TuneConfig`], [`SweepSpec`], [`ScoreWeights`] |
| [`task`] | [`TuneTask`], [`TaskCategory`], [`ExpectedOutcome`], [`ExpectedCall`], [`TaskSuite`] |
| [`result`] | [`TuneCandidateResult`], [`CandidateSource`], [`TuneTaskResult`] |

# Task Categories

Four are modeled after the Berkeley Function Calling Leaderboard (BFCL):
`single_call`, `parallel_call`, `multi_turn`, and `irrelevance` (correctly
abstaining from a tool call). A fifth, `long_context`, is gglib-specific:
it pre-fills [`TuneTask::history`](task::TuneTask::history) with a long
simulated prior conversation before `user_prompt`, testing whether context
degradation over a long session (the model's attention fixating on stale
context) causes it to loop or stagnate on a task it would otherwise handle
cleanly from a cold start — the failure mode this whole feature exists to
catch for agentic coding use.

# Task Schema

A [`TaskSuite::Custom`](task::TaskSuite::Custom) is a JSON array of
[`TuneTask`](task::TuneTask) values. The **same schema** is accepted from
the CLI (`--task-suite path.json`, parsed locally) and the GUI (parsed
client-side from an uploaded file, posted as the request body) — there is
one shared Serde shape, not two divergent ingestion paths.

# Scoring Methodology

Tool-call matching follows the Berkeley Function Calling Leaderboard (BFCL)
approach: structural (AST-style) comparison of the recorded call's name and
required arguments against [`ExpectedCall`](task::ExpectedCall), not a
string diff. Extra arguments the model supplies are ignored; argument
order never matters; call order only matters when a task's expected call
sets `ordered: true`.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`config.rs`](config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-config-coverage.json) |
| [`result.rs`](result.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-result-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-result-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-result-coverage.json) |
| [`task.rs`](task.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-task-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-task-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-task-coverage.json) |
<!-- module-table:end -->

</details>
