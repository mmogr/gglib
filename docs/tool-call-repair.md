# Tool-call repair

**Status:** implemented. Streaming and non-streaming both route through the hold-back.
**Decided by:** [ADR 0002](adr/0002-defer-tool-call-constraint-to-llama-cpp.md), findings 4–5.

## What this solves

`tool_choice: "auto"` is the path every agentic client uses, and on some
model/build pairs llama.cpp installs no grammar for it. Measured on `b10327`:

| model | `auto` conformance | `required` conformance | grammar under `auto` |
|---|---|---|---|
| Qwen3.5-4B | 30/30 | 30/30 | lazy grammar |
| Llama 3.2 3B | **≤ 4/30** | 30/30 | **none** |

On Llama 3.2 the model returns `max_lines: "42"` — a string where the schema
declares an integer — on 26 of 30 calls. The client's executor then fails,
reports the error back to the model, and the model tries again. That loop is
one of the ways a local agentic session dies, and today nothing in gglib
notices it happening.

The remedy is in the same table. Where `auto` is unconstrained and wrong,
`required` is constrained and right, on the same model in the same run. So
repair is not "build a grammar" — it is "ask upstream to use the one it
already has".

## The loop

```
                    ┌──────────────────────────────┐
   client request   │  request pipeline (existing) │
   tool_choice=auto │  stages 1-6, unchanged       │
        │           └──────────────┬───────────────┘
        │                          ▼
        │                    llama-server
        │                          │
        │                   tool_calls[]
        │                          ▼
        │           ┌──────────────────────────────┐
        │           │  validate against tools[]    │   ← Tier B, gglib-core
        │           │  schema (pure, no I/O)       │
        │           └──────────────┬───────────────┘
        │                 valid    │    invalid
        │                    ┌─────┴─────┐
        │                    ▼           ▼
        │                 forward    re-issue same messages
        │                            with tool_choice=required
        │                                 │
        │                                 ▼
        │                          llama-server (grammar installed)
        │                                 │
        │                          ┌──────┴──────┐
        │                     valid│             │still invalid
        ▼                          ▼             ▼
     client                    forward       forward ORIGINAL
                               repaired      (fail-open, recorded)
```

## Module placement

Follows the workspace's existing dependency direction — the decision is pure
and lives in core, the I/O lives in the adapter that already owns forwarding.

| Piece | Crate | Why there |
|---|---|---|
| `ToolCallValidator` — `(tools, tool_calls) -> Verdict` | `gglib-core::request_pipeline::validate` | Pure function over `serde_json::Value`. No HTTP, no runtime. Testable exhaustively against schema fixtures. |
| `RepairPolicy` — should this verdict be repaired, and how | `gglib-core::request_pipeline::validate` | Policy is domain. Keeps the "when" answerable without a server. |
| Repair executor — re-issue, swap, record | `gglib-proxy::forward` | Only the proxy can issue a second upstream request. |
| Counters / dashboard fields | `gglib-proxy::metrics` | Same shape as `dialect_residue_total` and `grammar_enforced`. |

Sitting the validator in `request_pipeline` beside `constrain` is deliberate:
they are the two halves of the same concern (make the call well-formed), and
one is being retired while the other takes over. Adjacency makes that legible.

## Validation

Recursive JSON Schema checking over each call's `arguments`, against the
`tools[]` entry with the matching `function.name`:

- **type** — including the `bool`-is-not-`integer` distinction
- **required** — presence of every declared key
- **enum** — membership
- **additionalProperties: false** — no invented keys
- **nested objects** — the same checks, recursively

That last one is not optional. The experiment harness checked nested *presence*
but not nested *types*, so `options: {"follow_symlinks": "null"}` passed a
validator that should have rejected it, and the reported Llama 3.2 conformance
rate was flattering. The production validator must recurse or it repeats that
error where it matters.

**Explicit non-goals.** No `$ref` resolution, no `anyOf`/`oneOf`/`allOf`, no
`pattern`, no recursion into `$defs`. A schema using any of them yields
`Verdict::Unvalidatable` and is forwarded untouched. gglib is not implementing
a JSON Schema engine; it is checking the constraint kinds small models
demonstrably get wrong.

```rust
pub enum Verdict {
    /// Every call conforms.
    Valid,
    /// At least one call violates the schema; carries what and where.
    Invalid(Vec<Violation>),
    /// Schema uses constructs this validator does not implement.
    Unvalidatable(&'static str),
    /// Request carried no tools, or response carried no calls.
    NotApplicable,
}
```

## When repair fires

All four must hold. Anything else forwards unchanged.

1. Verdict is `Invalid`.
2. The **original request's `tool_choice` was `auto`** (or absent). If the
   client already asked for `required` and the result is still invalid,
   re-issuing with `required` changes nothing — the grammar was already
   installed and the violation is something it does not cover.
3. The response is not already a repair attempt. **One attempt, never a loop.**
4. Repair is enabled (`Settings.tool_call_repair`, default on, with a
   `GGLIB_DISABLE_TOOL_REPAIR` env kill switch matching the convention of
   `GGLIB_DISABLE_GRAMMAR` and `GGLIB_DISABLE_AGENTIC_SAMPLING`).

Per ADR 0002 we validate *every* applicable response rather than gating on a
pre-emptive per-model detector. Validation is a schema walk over a few hundred
bytes; a detector would be a second thing to keep correct for no measurable
saving.

## The repair request

Identical to the original except `tool_choice: "required"`. Same messages, same
sampling, same model, same session. Rationale for each choice:

- **Same messages.** The prefix is unchanged, so the prompt cache serves the
  prefill and the second generation costs decode only.
- **Same sampling.** Changing temperature as well would confound which change
  produced the improvement, and the grammar is doing the work.
- **`required` is semantically safe here.** Forcing a call normally overrides a
  model's judgement about whether to call at all — but on this path the model
  *already emitted a call*. We know the intent; we are fixing the shape.

### One interaction that must not be missed

`request_pipeline::constrain` (stage 6) fires on `tool_choice: "required"` for
dialect models, installs gglib's own grammar, and **rewrites `tool_choice` to
`"none"`** — because llama-server rejects a custom grammar combined with
`tools`. If the repair request goes through the pipeline unchanged, stage 6
converts it into a request that asks for no tool call at all, and repair
silently does nothing.

The repair request must therefore carry a marker that suppresses stage 6, so
upstream's grammar is the one that fires. This is the whole point of the
mechanism: gglib's grammar is weaker than upstream's, and on this path we want
upstream's.

## When repair fails

Forward the **original** response, unmodified, and record the failure.

Never error the request, never forward a half-repaired response, never retry
again. This matches the fail-open discipline the proxy already applies in
truncation (unparseable body → forward unchanged) and the loop guard
(malformed tool arguments → hash the raw string rather than reject). A
protection that can make the outcome worse than its absence is not a
protection.

## Streaming

The hard part, and the one place this design constrains the response path.

A repair decision cannot be made until the tool call is complete, and by then a
naive streaming proxy has already sent the arguments to the client. You cannot
un-send them.

**Resolution: hold back tool-call deltas, stream everything else.** Text
content and reasoning stream normally with no added latency. `tool_calls`
deltas are buffered until `finish_reason` arrives, validated, then emitted —
either the original or the repaired call.

**The re-issue itself is non-streaming.** A repair cannot be judged until the
call is complete, so streaming it would buy no latency while requiring a second
SSE pipeline — decoder, normalizer, encoder, `[DONE]` bookkeeping — to run
inside the first. The buffered body is parsed once and synthesized back into
`ToolCallDelta` events, so every frame the client sees still flows through the
one `SseEncoder` that has been encoding the turn all along.

**Ordering.** Held frames are flushed *before* the `Done` frame, never after: a
client that sees `finish_reason` first considers the turn over. The trailing
`Usage` frame and the single `[DONE]` sentinel are untouched by the hold-back
and keep their existing ordering, which is what stops a client parser choking
on a repaired turn.

**One accepted wart.** A turn that emits text *then* a bad tool call will show
the client attempt 1's text followed by attempt 2's call. The text was the
model's preamble and the call is now correct, which beats the alternative; and
under `tool_choice: "required"` the re-issue emits no text of its own. Measured
turns on the `auto` path emitted empty content anyway.

This is acceptable because a tool call is not consumable incrementally: no
agentic client can act on half a call, and every one of them reassembles the
deltas before dispatching. The added latency applies only to the tool-call
portion of a turn, and only up to the point it would have been usable anyway.

The alternative — repair only on `stream: false` — is rejected. Every agentic
client streams, so it would ship a feature that never runs, which is exactly
the inert-Tier-A trap ADR 0002 flagged.

## Observability

Every repair is a fact about a model/build pair, which makes this Tier C data
as much as Tier B behaviour:

- `tool_calls_validated`, `tool_calls_repaired`, `tool_calls_repair_failed` on
  the dashboard snapshot, alongside `dialect_residue_total`.
- `ContextSnapshot` gains `tool_call_repaired`, back-patched after the response
  completes — the same pattern `dialect_residue` uses.
- A `warn!` on repair failure naming the model, the violation kinds, and the
  llama.cpp build.

A model that repairs constantly is evidence its `auto` path is unconstrained,
which is precisely the per-model grammar-presence data ADR 0002 lists as a
follow-up and which nothing can currently query at runtime. The repair counter
is how that gets measured in production rather than in a `--verbose` log.

## Testing

- Validator: table-driven over schema/arguments pairs, one case per constraint
  kind, plus the nested-type case the harness got wrong.
- Repair trigger: unit tests over `(verdict, original tool_choice, is_retry)`
  asserting fires/does-not-fire.
- Stage-6 suppression: a test that a repair request survives the pipeline with
  `tool_choice: "required"` intact — the interaction above, pinned so a future
  change to `constrain` cannot silently disable repair.
- Streaming: integration test that tool-call deltas are withheld until
  `finish_reason` and that text deltas are not.
- End-to-end: replay a recorded Llama 3.2 `auto` response with
  `max_lines: "42"`, assert repair fires and the forwarded call conforms.

## Cost

One extra generation per invalid call, decode-only (prefix cached). On Llama
3.2's measured rate that is roughly 0.87 extra generations per tool call, which
is a real cost and cheaper than the executor-error round trip it replaces —
that one costs a generation *plus* a tool round trip *plus* the context growth
of an error message the model then has to reason about.

On Qwen3.5 it costs nothing, because nothing fails validation.

## What this is not

- Not semantic repair. A call with `path: "src/mian.rs"` is schema-valid and
  wrong, and stays wrong. Executor-feedback repair is a separate, larger
  feature.
- Not a JSON Schema engine.
- Not grammar origination. That work was dropped in ADR 0002 and this design
  exists partly to make its absence survivable.
