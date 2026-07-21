# Request Pipeline

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-complexity.json)

<!-- module-docs:start -->

Shared request-shaping inputs for every inference pipeline.

`gglib` has two request paths that historically diverged: `gglib proxy`, which
applied a full shaping pipeline, and the agent path used by `gglib chat`,
`gglib q`, the web UI and council, which applied almost none of it. Both start
from the same question — *what do we know about this model?* — and this module
is the one place that answers it.

## Module map

- [`model_context`] — [`ModelContext`], the resolved per-model facts
  (capabilities, `format:*` tags, inference defaults) that the request and
  response stages are built from, plus the inert
  [`ModelContext::passthrough`] fallback.
- [`resolve`] — [`resolve()`], the single catalog round-trip that produces one.

## Why the three fields travel together

They feed three different stages — capabilities drive request-side transforms,
tags drive response-parser selection, defaults are the per-model layer of the
sampling hierarchy — but they all come from one catalog row. Resolving them
separately is what produced the split-brain this module exists to close.

Identifier resolution itself is not decided here: [`resolve()`] goes through
[`crate::ports::ModelCatalogPort`], whose implementations delegate to
[`crate::ports::ModelRepository::get_by_identifier`] — the workspace's single
lookup-key policy.

## Fallback policy

Exactly one, applied by [`resolve()`]: an unresolvable model yields
[`ModelContext::passthrough`], so it loses its model-specific handling and
nothing else. Unknown models log at `debug` (routine — clients name models the
catalog has never seen); catalog errors log at `warn` (something is broken).

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`model_context.rs`](model_context.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-model_context-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-model_context-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-model_context-coverage.json) |
| [`resolve.rs`](resolve.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-resolve-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-resolve-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-resolve-coverage.json) |
<!-- module-table:end -->

</details>
