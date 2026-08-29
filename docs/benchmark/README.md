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
