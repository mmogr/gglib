# council

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-council-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-council-complexity.json)

<!-- module-docs:start -->

Orchestrator domain model.

This module owns the pure data types that drive the Director/Worker
orchestration pattern.  No I/O, no async, no adapter dependencies.

# Submodules

| Module | Contents |
|--------|---------|
| [`task_graph`] | [`TaskGraph`], [`TaskNode`], [`TaskNodeKind`], [`NodeId`], [`NodeStatus`], [`HitlMode`], [`TaskGraphError`] |
| [`role_catalog`] | [`RoleId`], [`RoleSpec`], [`RoleCatalog`] — built-in specialist roles |
| [`events`] | [`CouncilEvent`] — SSE event stream types |
| [`run`] | [`CouncilRun`], [`CouncilRunStatus`], [`CouncilRunEvent`] |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`events.rs`](events.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-events-coverage.json) |
| [`role_catalog.rs`](role_catalog.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-role_catalog-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-role_catalog-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-role_catalog-coverage.json) |
| [`run.rs`](run.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-run-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-run-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-run-coverage.json) |
| [`task_graph.rs`](task_graph.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-task_graph-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-task_graph-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-council-task_graph-coverage.json) |
<!-- module-table:end -->

</details>
