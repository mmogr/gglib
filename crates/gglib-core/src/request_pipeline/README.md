# Request Pipeline

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-complexity.json)

<!-- module-docs:start -->

Request shaping for every inference pipeline: what we know about the model, and
what we do to the request because of it.

`gglib` has two request paths that historically diverged: `gglib proxy`, which
applied a full shaping pipeline, and the agent path used by `gglib chat`,
`gglib q`, the web UI and council, which applied almost none of it. Both start
from the same question — *what do we know about this model?* — and both need
the same answer applied to the outgoing body. This module is the one place that
does either.

## Module map

**Resolution — what the model is**

- [`model_context`] — [`ModelContext`], the resolved per-model facts
  (capabilities, `format:*` tags, inference defaults) that the request and
  response stages are built from, plus the inert
  [`ModelContext::passthrough`] fallback.
- [`resolve`] — [`resolve()`], the single catalog round-trip that produces one.

**Shaping — what happens to the request**

- [`apply`] — [`apply()`], the whole ordered pipeline as one call, and the one
  place the stage order and its rationale are written down. **Read this first.**
- [`messages`] — [`shape_messages()`], stages 1–2: reasoning strip and
  capability coalescing. Everything that rewrites the `messages` array.
- [`sampling`] — [`resolve_sampling()`] and [`SamplingLayers`], stages 4–5: the
  sampling hierarchy and the `cache_prompt` pin. Everything that touches
  top-level keys.

Stage 3, history truncation, is still `gglib-proxy`-local — it gates on the
payload's size in wire bytes and can reject the request with an HTTP response,
neither of which fits this crate. The proxy therefore calls
[`shape_messages()`] and [`resolve_sampling()`] with its own truncation pass
between them, rather than calling [`apply()`]; a test in [`apply`] pins the two
routes to the same result.

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

Shaping inherits it for free: a passthrough context has empty capabilities, so
every message-level stage is a no-op, and no per-model defaults, so the
sampling hierarchy simply resolves one layer shallower. An unknown model never
costs the request itself.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`apply.rs`](apply.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-apply-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-apply-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-apply-coverage.json) |
| [`messages.rs`](messages.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-messages-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-messages-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-messages-coverage.json) |
| [`model_context.rs`](model_context.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-model_context-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-model_context-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-model_context-coverage.json) |
| [`resolve.rs`](resolve.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-resolve-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-resolve-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-resolve-coverage.json) |
| [`sampling.rs`](sampling.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-sampling-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-sampling-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-sampling-coverage.json) |
<!-- module-table:end -->

</details>
