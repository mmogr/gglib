# Tool-call repair

**Status:** implemented on both paths. A streamed turn holds its tool-call
deltas back until the call is complete, judges it and re-issues; a
`stream: false` turn has nothing to hold back, since its body is buffered
anyway, so it is judged once read and re-issued the same way. Both re-issues
are non-streaming, and both answers are read through the model's dialect
before they are judged. `RepairContext` carries the decision on the streaming
path (`sse_stream.rs`); the buffered path hands the same `RepairTurn` to
`forward_unary`.
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
        │                            (required, or a second draw)
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
| Repair executor — re-issue, swap, record | `gglib-proxy::forward` (streamed) and `gglib-proxy::forward_unary` (buffered), sharing `repair::read_second_draw` | Only the proxy can issue a second upstream request. |
| Counters / dashboard fields | `gglib-proxy::metrics` | Same shape as `dialect_residue_total` and `grammar_enforced`. |

Sitting the validator in `request_pipeline` beside `constrain` is deliberate:
they are the two halves of the same concern (make the call well-formed), and
one is being retired while the other takes over. Adjacency makes that legible.

## Validation

Recursive JSON Schema checking over each call's `arguments`, against the
`tools[]` entry with the matching `function.name`:

- **type** — including the `bool`-is-not-`integer` distinction, and a list of
  types, as an optional field's `["string", "null"]`. A type name the
  validator does not know passes, with a debug line naming it
- **required** — presence of every declared key
- **enum** — membership
- **additionalProperties: false** — no invented keys
- **nested objects** — the same checks, recursively

That last one is not optional. The experiment harness checked nested *presence*
but not nested *types*, so `options: {"follow_symlinks": "null"}` passed a
validator that should have rejected it, and the reported Llama 3.2 conformance
rate was flattering. The production validator must recurse or it repeats that
error where it matters.

**Explicit non-goals.** No `$ref` resolution, no `anyOf`/`oneOf`/`allOf`/`not`,
no recursion into `$defs`, and no `prefixItems`, since `items` is applied to
every element. A schema using any of them yields
`Verdict::Unvalidatable` and is forwarded untouched. Those keywords are looked
for only where a schema puts keywords, so a parameter named `if` or
`definitions` is validated like any other. `pattern` is not checked at all.
gglib is not implementing a JSON Schema engine; it is checking the constraint
kinds small models demonstrably get wrong.

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
   installed and the violation is something it does not cover. The
   exception is a turn gglib's own grammar constrained (below): that grammar
   is weaker than upstream's, so its call is judged, and a violation is drawn
   again under the same grammar.
3. The response is not already a repair attempt. **One attempt, never a loop.**
4. Repair is enabled (`Settings.tool_call_repair`, default on, with a
   `GGLIB_DISABLE_TOOL_REPAIR` env kill switch matching the convention of
   `GGLIB_DISABLE_GRAMMAR` and `GGLIB_DISABLE_AGENTIC_SAMPLING`).

   Settable as `gglib config settings set --tool-call-repair false`, or from
   Settings → Advanced in the app. Turn it off to measure what a model
   actually produces: with repair on, a model that packages tool calls badly
   looks like a model that does not, because the proxy fixes it before the
   client sees it — which is the right default for using one and the wrong
   one for judging one.

Per ADR 0002 we validate *every* applicable response rather than gating on a
pre-emptive per-model detector. Validation is a schema walk over a few hundred
bytes; a detector would be a second thing to keep correct for no measurable
saving.

## The repair request

On an `auto` turn, identical to the original except `tool_choice: "required"`;
on a turn gglib's own grammar constrained, the forwarded body again (below).
Same messages, same sampling, same model, same session. Rationale for each
choice:

- **Same messages.** The prefix is unchanged, so the prompt cache serves the
  prefill and the second generation costs decode only.
- **Same sampling.** Changing temperature as well would confound which change
  produced the improvement, and the grammar is doing the work.
- **`required` is semantically safe here.** Forcing a call normally overrides a
  model's judgement about whether to call at all — but on this path the model
  *already emitted a call*, so asking for one again takes nothing from it.

**The re-issue is a fresh generation, not a correction.** Nothing carries the
first call's function name or arguments into the second request: the model is
asked the same question again under a stronger constraint (or, on a turn
gglib's own grammar constrained, under the same one), and it may answer with a
different tool, or the same tool with different arguments, not only the same
call in a valid shape. What repair guarantees is that the call the client
receives validates against the schema, not that it is the call the model first
meant. The client never saw the held-back call, so it has nothing to undo; but
text the model streamed before the call, which the client did see, may no
longer match the call that follows it.

### One interaction that must not be missed

`request_pipeline::constrain` (stage 6) fires on `tool_choice: "required"` for
dialect models, installs gglib's own grammar, and **rewrites `tool_choice` to
`"none"`** — because llama-server rejects a custom grammar combined with
`tools`. If the repair request goes through the pipeline unchanged, stage 6
converts it into a request that asks for no tool call at all, and repair
silently does nothing.

The repair request therefore never goes back through the pipeline: it is the
forwarded body with `tool_choice` set to `"required"`, sent as it is, so
upstream's grammar is the one that fires.

### A turn gglib's own grammar constrained

When stage 6 installed gglib's grammar, the forwarded body already carries it,
with `tool_choice: "none"`, and llama-server refuses a custom grammar beside
`required`. So the re-issue cannot switch to upstream's grammar. It is the same
body sent again, non-streaming: a second draw under the same constraint, not a
stronger one, and it may choose a different call. A draw that repeats the same
call gains nothing, and `choose` then keeps the original.

llama-server parses no tool calls under `tool_choice: "none"`, so the second
draw comes back as the model's dialect markup in `content`. It is read the way
a non-streaming response is, by the same dialect parser, before it is judged.
Any re-issue's answer is read that way; for a model with no dialect, that
changes nothing.

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
under `tool_choice: "required"` the re-issue emits no text of its own, and a
second draw under gglib's grammar answers in markup that is read as the call,
not as text. Measured turns on the `auto` path emitted empty content anyway.

This is acceptable because a tool call is not consumable incrementally: no
agentic client can act on half a call, and every one of them reassembles the
deltas before dispatching. The added latency applies only to the tool-call
portion of a turn, and only up to the point it would have been usable anyway.

The alternative — repair only on `stream: false` — is rejected. Every agentic
client streams, so it would ship a feature that never runs, which is exactly
the inert-Tier-A trap ADR 0002 flagged.

### Non-streaming

A `stream: false` turn needs none of the above. The body is buffered before
anything is answered, so there is nothing to hold back and nothing to
interleave: `forward_unary` reads the body, runs it through the dialect parser
once, judges the first choice's call and, on a violation, sends the same
re-issue the streaming path would — `tool_choice: "required"` on an `auto`
turn, a second draw under the same grammar on a turn stage 6 constrained.
From the send onward the two paths share `repair::read_second_draw`, which
reads the answer through the dialect before `choose` judges it. The client is
answered with whichever body validates, the original when the draw does not,
and the dashboard's repair counters and the per-model ledger record the
attempt as they do for a streamed turn.

What this path cannot do is keep the wire warm. The streaming path pushes an
SSE comment every fifteen seconds while a re-issue is in flight; a buffered
response has no frame to push, so the client hears nothing until the second
generation ends, bounded by the same sixty-second re-issue cap. See "Cost".

## Observability

Every repair is a fact about a model/build pair, which makes this Tier C data
as much as Tier B behaviour:

- `tool_calls_validated`, `tool_calls_repaired`, `tool_calls_repair_failed` on
  the dashboard snapshot, alongside `dialect_residue_total`.
- `ContextSnapshot` gains `tool_call_repaired`, back-patched after the response
  completes — the same pattern `dialect_residue` uses.
- A `warn!` on repair failure naming the model, the violation kinds, and the
  llama.cpp build.
- On a `stream: false` turn the dialect-residue flag describes the first
  answer, which is what was normalised, even when a repair then answers the
  client with a second draw; the streamed path's flag describes the frames
  the client received. Telemetry only, and the overlap (residue, a violation
  and a successful draw on one turn) is narrow.

A model that repairs constantly is evidence its `auto` path is unconstrained,
which is precisely the per-model grammar-presence data ADR 0002 lists as a
follow-up and which nothing can currently query at runtime. The repair counter
is how that gets measured in production rather than in a `--verbose` log.

## Testing

- Validator: table-driven over schema/arguments pairs, one case per constraint
  kind, plus the nested-type case the harness got wrong.
- Repair trigger: unit tests over `(verdict, original tool_choice, is_retry)`
  asserting fires/does-not-fire.
- Stage-6 bypass: `the_pipeline_would_destroy_a_repair_body_which_is_why_it_bypasses_it`
  asserts that stage 6 would rewrite a repair body's `tool_choice` to `"none"`,
  which is why the re-issue never goes back through the pipeline; the
  second-draw tests pin the turn stage 6 did constrain.
- Streaming: integration test that tool-call deltas are withheld until
  `finish_reason` and that text deltas are not.
- Non-streaming, end to end (`forward_unary_repair_tests.rs`): a bad call is
  re-issued and the client gets the valid one; a turn gglib's grammar
  constrained is answered in markup and read as a call; a valid call is not
  re-issued; a re-issue upstream rejects falls open to the first answer and
  counts as an attempt; repair off leaves the body as it came.
- End-to-end: replay a recorded Llama 3.2 `auto` response with
  `max_lines: "42"`, assert repair fires and the forwarded call conforms.

## Cost

One extra generation per invalid call, decode-only (prefix cached). On Llama
3.2's measured rate that is roughly 0.87 extra generations per tool call, which
is a real cost and cheaper than the executor-error round trip it replaces —
that one costs a generation *plus* a tool round trip *plus* the context growth
of an error message the model then has to reason about.

On Qwen3.5 it costs nothing, because nothing fails validation.

On a `stream: false` turn the worst case doubles the wait: the first
generation, then up to sixty seconds of re-issue with no keepalive the client
can hear. A client whose own deadline is shorter than that sum gives up on a
repaired turn it would have accepted unrepaired, and nothing here can tell it
a second generation is under way. Every agentic client streams, so this is
the rare path; it is written down because a client that hits it sees a
timeout, not a repair.

## Measured through a real proxy (2026-09-20)

Everything above was measured against llama-server, or in the proxy's own
tests. The agentic benchmark's proxy arm (#1047) sends every turn through a
real proxy, so repair can be read end to end. The first run, on a
schema-stress suite of six tasks and three seeds, against `b619935e`:

| model | repairs attempted / succeeded | tool accuracy, no proxy → through it |
|---|---:|---|
| Llama 3.2 3B Q8_0 | 11 / 11 | 0.639 → 1.000 |
| Qwen3.8-27B Q8_0 | 0 / 0 | 1.000 → 1.000 |

On Llama 3.2 the proxy scored higher on tool-match score in 8 of 15 matched
`(task, seed)` pairs and lower in none. The arm-level delta is withheld,
because three of that pair's runs never reached the model. The 8 are the proxy
arm's, not repair's: they are everything the proxy does to a request and its
answer.
The 2026-09-20 addendum in
[ADR 0004's log](adr/log-0004.md#addendum--the-first-reading-through-the-proxy-2026-09-20)
has the reading, the axis that moved the other way, and what none of it
licenses.

## What shrinks this mechanism

This module is Tier B in [ADR 0001](adr/0001-runtime-capability-tiers.md)'s
terms — policy, which that ADR asks no deletion criterion of, unlike Tier A.
Judging a call against the schema the client sent, and re-issuing rather than
handing the client something its executor will reject, is gglib's and stays.
What can shrink is how often either half has anything to fire on, and the two
halves retire separately:

- The `auto` path exists because llama.cpp installs no grammar for
  `tool_choice: "auto"` on some model/build pairs. No criterion deletes it
  outright; it stops firing model by model as chat templates constrain more of
  them.
- The second draw under gglib's own grammar exists because stage 6 constrains
  a dialect model more weakly than upstream's own grammar would. It goes when
  stage 6 goes, on the deletion criterion `request_pipeline::constrain`
  already carries.

None of that is visible in a score, because a repaired call reaches the client
as the repaired call. `repairs_attempted` in the report is what anyone will
notice it by — but a zero is a reason to look rather than a reading, because
several unlike facts produce one: a model that broke no schema, a model whose
`auto` turns upstream already constrains, a client that asked for `required`
on a turn gglib's own grammar did not constrain, a turn with no tools in it or
none called. `Skipped` names them apart in the decision; the counter does not.
Qwen3.8-27B's zero above is one of the first two — nothing it emitted failed
validation — and the run does not say which.

## What this is not

- Not semantic repair. A call with `path: "src/mian.rs"` is schema-valid and
  wrong, and stays wrong. Executor-feedback repair is a separate, larger
  feature.
- Not a JSON Schema engine.
- Not grammar origination. That work was dropped in ADR 0002 and this design
  exists partly to make its absence survivable.
