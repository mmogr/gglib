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
  (capabilities, `format:*` tags, inference defaults, context length) that the
  request and response stages are built from, plus the inert
  [`ModelContext::passthrough`] fallback.
- [`resolve`] — [`resolve()`], the single catalog round-trip that produces one.

**Shaping — what happens to the request**

- [`apply`] — [`apply()`], the whole ordered pipeline as one call, and the one
  place the stage order and its rationale are written down. **Read this first.**
- [`messages`] — [`shape_messages()`], stages 1–2: reasoning strip and
  capability coalescing. Everything that rewrites the `messages` array.
- [`truncation`] — [`truncate_history()`], stage 3: trimming stale tool results
  and oversized assistant turns to fit the model's context budget, and
  rejecting the request when it cannot be made to fit.
- [`sampling`] — [`resolve_sampling()`] and [`SamplingLayers`], stages 4–5: the
  sampling hierarchy and the `cache_prompt` pin. Everything that touches
  top-level keys.

Every request path calls [`apply()`]. The proxy used to run the stages by hand
with its own truncation pass spliced between them, because that pass gated on
the payload's size in wire bytes and could reject the request with an `axum`
response — neither of which fits here. Measuring the serialized `Value` and
returning a domain error removed both obstacles, so there is one implementation
of the order and no second route to keep in sync.

## The truncation budget

Stage 3 needs a character budget, and it comes from the model:
[`ModelContext::context_budget_chars`] converts the model's context length at
[`CHARS_PER_TOKEN_APPROX`]. There is no floor — a 4,096-token model gets a
~16,000-character budget and a 262,144-token model gets a ~1,000,000-character
one — so the same conversation is treated differently on different models,
which is the point.

Callers holding better information pass their own number instead. Only one
does: `gglib-proxy` knows the **live** serving context of the running
llama-server and learns a per-model chars-per-token ratio from observed usage
frames. That calibration is stateful and tied to the proxy's request lifecycle,
so it stays there.

`None` means *do not truncate*, not *truncate at zero*. An unresolvable model
has no context length, and guessing one would risk rejecting a request over a
number nobody knows.

## Why the fields travel together

They feed four different stages — capabilities drive request-side transforms,
tags drive response-parser selection, defaults are the per-model layer of the
sampling hierarchy, context length is the truncation budget — but they all come
from one catalog row. Resolving them separately is what produced the
split-brain this module exists to close.

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
| [`truncation.rs`](truncation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-truncation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-truncation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-truncation-coverage.json) |
| [`truncation_tests.rs`](truncation_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-truncation_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-truncation_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-truncation_tests-coverage.json) |
<!-- module-table:end -->

</details>
