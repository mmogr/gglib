# Request Pipeline

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-request_pipeline-complexity.json)

<!-- module-docs:start -->

Request shaping for every inference pipeline: what we know about the model, and
what we do to the request because of it.

`gglib` has two request paths that historically diverged: `gglib proxy`, which
applied a full shaping pipeline, and the agent path used by `gglib chat`,
`gglib q` and the web UI, which applied almost none of it. Both start
from the same question — *what do we know about this model?* — and both need
the same answer applied to the outgoing body. This module is the one place that
does either.

## Module map

**Routing — which model the request names**

- [`profile_route`] — [`resolve_route()`], which reads a `{model}:{profile}`
  suffix off a requested id and decides whether it names a model outright, a
  model plus a configured profile, or a profile that does not exist.

  This one runs *before* the pipeline rather than inside it. Every stage of
  [`apply()`] shapes a request already known to belong to some model;
  [`resolve()`] cannot even build a [`ModelContext`] until the base name is
  known, and stripping the suffix is what produces that name. So the order for
  a caller that supports profiles is: route, then resolve the base name it
  returns, then apply. A caller that does not support profiles skips straight
  to [`resolve()`], which is why this is a separate entry point rather than a
  stage — and why an id with no `:` costs no catalog access at all.

**Resolution — what the model is**

- [`model_context`] — [`ModelContext`], the resolved per-model facts
  (capabilities, `format:*` tags, inference defaults, context length) that the
  request and response stages are built from, plus the inert
  [`ModelContext::passthrough`] fallback.
- [`mod@resolve`] — [`resolve()`], the single catalog round-trip that produces one,
  and [`resolve_summary()`], the same lookup for a caller that refuses a model
  the catalog does not hold rather than degrade it.
- [`request_shape`] — [`carries_tools()`], the one thing the stages need to know
  about the *request* rather than the model: whether it is asking for a tool
  call. Read by two stages for different purposes, so it lives in neither.
- [`content`] — [`text_len()`] and [`for_each_text_mut()`], the two shapes a
  message's `content` takes (a string, or an array of parts), read and
  rewritten in one place for every stage that handles message text; and
  [`image_urls()`], the one walk over a message's `image_url` parts.
- [`images`] — the one image policy. [`estimate_image_tokens()`] prices an
  image by its pixels (one token per 32x32 square, at most 4,096), and
  [`image_url_tokens()`] prices a URL, at the cap when its size cannot be
  read. [`refuse_unless_can_see()`] refuses an image for a model with no
  projector, and holds the error code and the remedy for every surface.
  [`MAX_IMAGE_BYTES`] (8 MiB) is the most one stored image may be, and
  [`MAX_REQUEST_IMAGE_BYTES`] (16 MiB) the most raw image bytes one request
  to a model may carry.
- [`mod@image_size`] — [`image_size()`] and [`data_url_image_size()`]: a PNG's or
  a JPEG's width and height from its header alone, decoding only a bounded
  prefix of a base64 payload. Input cut short or not an image is `None`.
  [`image_mime()`] is its media type, by its first bytes.

**Shaping — what happens to the request**

- [`mod@apply`] — [`apply()`], the whole ordered pipeline as one call, and the one
  place the stage order and its rationale are written down. **Read this first.**
- [`messages`] — [`shape_messages()`], stages 1–2: reasoning strip and
  capability coalescing. Everything that rewrites the `messages` array.
- [`truncation`] — [`truncate_history()`], stage 3: trimming stale tool results
  and oversized assistant turns to fit the model's context budget, and
  rejecting the request when it cannot be made to fit.
- [`truncation_parts`] — `elide()`, the elision of one message in either
  content shape; kept beside `truncation`, which is at its file budget.
- [`measure`] — [`ContextBudget`], the characters a request may measure and
  the context in tokens they stand for, and the measurement stage 3 takes: a
  request's serialized length, with each image counted at its estimated
  tokens in place of the length of its URL.
- [`sampling`] — [`resolve_sampling()`] and [`SamplingLayers`], stages 4–5: the
  sampling hierarchy, the floor selection (neutral / reasoning / tool-call), and
  the `cache_prompt` pin. Everything that touches top-level keys.
- [`effort_gate`] — stage 5b: deleting a resolved `reasoning_effort` the
  model's observed template does not read, and writing down what was deleted so
  a surface can say whose setting went nowhere.
- [`sampling_log`] — no stage of its own; the single `sampling resolved` debug
  line, rendered after 5b so it describes what was *sent* rather than what
  stage 4 folded. Its module docs carry the argument for that placement.

Outside tests, two paths call [`apply()`]: the proxy's forwarding path
(`gglib-proxy`'s `forward.rs`) and the runtime's completion adapter
(`gglib-runtime`'s `llm_completion`), and neither runs the stages in an order
of its own. `POST /api/chat` in `gglib-axum` does not call it and runs none of
the stages: it posts straight to llama-server, as the note in its handler,
`proxy_chat` in `chat_api.rs`, says.

## The truncation budget

Stage 3 needs a [`ContextBudget`], and it comes from the model:
[`ModelContext::context_budget`] converts the model's context length at
[`CHARS_PER_TOKEN_APPROX`], and keeps the token count beside the characters
so an image's tokens can be counted at the same ratio. There is no floor — a 4,096-token model gets a
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
