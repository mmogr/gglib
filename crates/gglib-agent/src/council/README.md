# council

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-complexity.json)

<!-- module-docs:start -->

Orchestrator Director — decompose goals into task graphs.

This module contains the director agent that translates a high-level goal
into a validated [`gglib_core::domain::council::task_graph::TaskGraph`]
of worker nodes.

# Modules

| Module | Contents |
|--------|----------|
| [`chief_of_staff`] | [`chief_of_staff::brief`] — decompose goal into department briefs |
| [`director`] | [`director::plan`] — flat director planning, [`director::PlanError`] |
| [`planner`] | [`planner::plan`] — hierarchical two-tier planning entry point |
| [`prompts`]  | System prompt templates, few-shot examples, JSON Schemas |

# Phase H scope

Phase H replaces the flat single-shot director with a two-tier hierarchical
planner.  The executor's external call signature is unchanged — it calls
[`planner::plan`] which internally runs Chief of Staff → N × Director.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`chief_of_staff.rs`](chief_of_staff.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-chief_of_staff-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-chief_of_staff-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-chief_of_staff-coverage.json) |
| [`compaction.rs`](compaction.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-compaction-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-compaction-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-compaction-coverage.json) |
| [`director.rs`](director.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-director-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-director-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-director-coverage.json) |
| [`estimator.rs`](estimator.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-estimator-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-estimator-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-estimator-coverage.json) |
| [`executor.rs`](executor.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-executor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-executor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-executor-coverage.json) |
| [`planner.rs`](planner.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-planner-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-planner-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-planner-coverage.json) |
| [`prompts.rs`](prompts.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-prompts-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-prompts-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-prompts-coverage.json) |
| [`spawn.rs`](spawn.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-spawn-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-spawn-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-spawn-coverage.json) |
| [`steering.rs`](steering.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-steering-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-steering-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-steering-coverage.json) |
| [`synthesis.rs`](synthesis.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-synthesis-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-synthesis-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-council-synthesis-coverage.json) |
| [`debate/`](debate/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-debate-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-debate-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-debate-coverage.json) |
<!-- module-table:end -->

</details>
