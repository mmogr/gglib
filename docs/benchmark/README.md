# Isolating one agentic task

The default agentic eval runs 21 tasks × 3 seeds × 4 arms — 210 model runs, and
the better part of an hour. That is the wrong instrument for asking "what did
the model actually do on *this* task", and the cost is enough that the question
usually goes unasked.

`--task-suite` takes a **file path** as readily as `default`, so a suite of one
task is a suite. Combined with `--seeds ""` (one unseeded run) and dropping the
two secondary arms, the same machinery runs the same task in **2 model runs**.

```
gglib benchmark agentic -m <model> \
  --task-suite docs/benchmark/one_task_planted_values.json \
  --seeds "" --no-control --no-replicate \
  --output one_task.json
```

Both real arms still run, so raw-versus-gglib is still a comparison — it is the
repeats and the sensitivity check that are dropped, and those are what make the
full suite slow. Nothing about the arms themselves changes, which is the point:
a task isolated this way behaves as it did in the full run.

## Which seed

`--seeds ""` names no seed. It is the cheapest form and the least faithful one:
the full eval seeds every run, so a behaviour reproduced unseeded was reproduced
under conditions the original never ran. To repeat the run that raised the
question, name the seed it used — `--seeds 12345`, the first of `DEFAULT_SEEDS`
(`domain/benchmark/agentic.rs`). Same two runs, same cost, and the sampler is
back where it was.

Both forms are pinned by tests: `--seeds ""` reaching the eval as *no seeds* is
a regression guard, because the empty string has to survive a comma splitter
that would otherwise hand an integer parser nothing to parse.

## Seeing inside the run

The report says what the model *achieved*. To see what it *did*, raise the log
level — the daemon inherits the environment it is spawned from, and appends to
`<data root>/logs/daemon.log`:

```
gglib daemon stop
RUST_LOG="warn,gglib_agent=debug,gglib_core::request_pipeline=debug" \
  gglib benchmark agentic -m <model> \
    --task-suite docs/benchmark/one_task_planted_values.json \
    --seeds "" --no-control --no-replicate --output one_task.json
```

Two lines carry most of what is worth knowing:

| line | what it settles |
| --- | --- |
| `sampling resolved … max_tokens=… from=…` | whether a generation limit reached the wire at all, and which layer supplied it. There is no floor for `max_tokens`: if the model row does not name one, none is sent and llama-server's `n_predict` default of `-1` applies. |
| `LLM response received content_len=… reasoning_len=… tool_call_count=… finish_reason=…` | per request: thinking versus answering, a tool-call explosion, and whether generation stopped naturally or hit a ceiling. |

`finish_reason` is deliberately **not** in the report yet. It is per-response and
the eval's result is per-run, and the honest ways to carry it are either partial
(tool-executing turns only, missing the final one) or route through a return
value the eval cannot read on an aborted run — which are the runs that matter
most. Until there is a reason to pay for that, the log is where it lives.

## `one_task_planted_values.json`

Lifted verbatim from `crates/gglib-core/assets/tune_default_suite.json`. Twelve
turns of a staging-deploy conversation plant four facts — service name, host,
port, health path — each beside an explicit anti-fact (`8443` "not 8080",
`/healthz` because "the default `/health` collides"), with two irrelevant
digressions interleaved. Then: *"Go ahead — register the service with everything
we agreed."* One tool, four required arguments, one call.

Despite the `long_context` category it is not a large prompt — about 2 KB of
history, roughly 500 tokens. The category means *distractor endurance*, not
context-window pressure. Worth knowing before attributing anything here to
context size: nothing in the default suite comes near a context limit.

This is the task that generated ~32,900 completion tokens per run through the
pipeline on 2026-08-29, against ~510 without it, and passed both ways.

## Through the proxy: `--proxy`

Neither the raw nor the gglib arm reaches `gglib-proxy`. Both post to
llama-server, and the gglib arm applies the request pipeline in-process. So
neither measures what only the proxy does, above all validating each tool call
against the schema the client sent and re-issuing a broken one.

`--proxy` adds an arm that does, and a baseline for it. The proxy arm sends
every turn through a real proxy, started in-process in front of the model the
eval already holds, just before the arm, and stopped just after it. Its
baseline, raw (auto), goes straight to llama-server and is the raw arm with
one difference, `tool_choice`. The proxy judges every call whose schema it
can judge on an `"auto"` turn, but on a `"required"` turn only when gglib's
own grammar constrained it, which it does for a dialect model. So both open
with `"auto"`, where raw and gglib open with `"required"`, and the two are
compared with each other and not with raw and gglib.

The report's "through gglib-proxy" block gives the pair's scores and delta.
The delta is everything the proxy does, its request pipeline included, so it
does not say which part moved a score. The block also gives what the proxy
counted: requests, and repairs attempted and succeeded. Read the counts first.
A repaired call reaches the agent as the repaired call, so the scores alone
cannot say whether repair ran. When the proxy attempts no repair (with repair
on, no call it could judge broke its schema), the pair shows nothing about
repair, and the report says so.

The block also counts the proxy's loop-guard interventions. The eval's own
agent runs the same loop detectors and ends a stuck run itself, so whether
that count can be non-zero in an eval is not shown.

A proxy arm in which no run reached the model ends the eval, as any other such
arm does: a column of scores with nothing measured under it is not reported.

## `schema_stress_suite.json`

Six single-call tasks built to be violated. Five are traps for a model; the
sixth is a trap for the validator:

| task | the trap |
| --- | --- |
| `schema_integer_not_string` | `max_lines` is an integer, and the prompt spells it out in words |
| `schema_enum_case` | `level` is one of four lower-case values, and the prompt says `WARNING` |
| `schema_nested_required` | a required object with required fields of its own, and an enum at each level |
| `schema_parameter_named_if` | a parameter named `if`, which a validator that mistakes names for keywords refuses to judge (#1046) |
| `schema_array_of_objects` | an array of objects, each with a required integer |
| `schema_deep_nesting` | an enum three objects deep |

Every schema uses only keywords the proxy's validator checks: `type`, `enum`,
`required`, `properties`, `items` and `additionalProperties: false`. That is
deliberate. A schema holding a keyword the validator refuses (`$ref`, `anyOf`
and the like) is forwarded unjudged, and the task would then look like a
model that never needed repair. A keyword it ignores (`pattern`, `minimum`) is
never checked, so a trap built on one would catch nothing.
`crates/gglib-core/tests/schema_stress_suite.rs` holds the suite to being
judgeable: each expected call is valid, and the same call missing one
required argument is invalid, not unjudgeable. It does not notice an ignored
keyword; keep them out by reading the schema.

```
gglib benchmark agentic -m <model> \
  --task-suite docs/benchmark/schema_stress_suite.json --proxy
```
