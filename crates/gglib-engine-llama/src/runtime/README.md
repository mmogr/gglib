# runtime

<!-- module-docs:start -->

gglib's model runtime on a device that cannot start llama-server.
[`InProcessRuntime`] is the `ModelRuntimePort` gglib-proxy admits each chat
through, over one engine and the shim that serves it.

An admission resolves the model through the catalog
(`ModelCatalogPort::resolve_for_launch`, which `CatalogPortImpl` answers from
`gglib.db`), fits its context with gglib-core's `fit_verdict`, and loads it at
that context unless it is held at that context already. The fit is the model
browser's: the largest rung of the context ladder whose KV cache, at the
default `Q8_0` types the engine then loads with, fits beside the weights in
the device's budget. The weights are read from disk, every shard
(`total_model_bytes`), as the browser reads an installed model's; a projector
or component the catalog counts beside them is never loaded, so it is not
counted. A context the request asks for (`num_ctx`) lowers the rung and never
raises it; the proxy's `default_ctx` is not read, since nobody on the device
chose it. Before anything is loaded, a model whose smallest rung does not fit
is refused with `ChatModelDoesNotFit` (507), one trained for fewer tokens than
the smallest rung with `SpawnFailed` naming its trained context, and one
whose fit cannot be judged with `SpawnFailed` naming the fact that is missing.

The budget is made once, by `device_budget::budget_from`, from the
`DeviceReading` the runtime starts with, and held for its life: every
admission is fitted against the same number, so a model's context does not
move between requests and an admission of the model held loads nothing.

One model is held. Admitting another frees it first, ending the reply it was
generating; the admission is detached, with no lease, since the engine serves
one reply at a time. Admissions and stops take turns, and a placement runs on
a task of its own, so a caller that stops waiting does not leave a model
loaded that the runtime does not know of.

The engine publishes each step as an `EngineState` on a tokio watch: `Idle`,
`Loading` with the fraction the decoder reports read, `Ready` with its
context, `Generating` with the same context while a reply runs, `Unloading`,
`Failed` (a load that failed or panicked; the next admission tries again), and
`Stopped`. A state that names a model gives its catalogue id beside the name
it is served under, since two models may share a name. Every load is given
the runtime's closing `Abort`, which `shutdown` raises: a load under way
stops at the decoder's next report of progress and
frees what it read. `shutdown` also raises the abort of the reply in flight,
and frees the model once the decoder's lock is free, which the load and the
reply's generation hold until they return; it then stops the shim and ends the
watch on `Stopped`, and the runtime admits nothing after.

# Structure

- `mod.rs` - [`InProcessRuntime`]: start, admission, stops, `subscribe` and
  `shutdown`
- `fit.rs` - the context a model is loaded with under the budget, or why it
  is refused, and the KV cache types it is loaded with
- `test_support.rs` - a runtime over a `ScriptedDecoder` and a catalog of
  launch specs, on a device with a 2 GiB budget, and a shelf that puts each
  model's weights on disk as a sparse file
- `fit_tests.rs` - the rung, a context asked for, too big, a model trained
  under the smallest rung, and a fit that cannot be judged
- `admit_tests.rs` - the rung reaching the decoder and the shim, a refusal
  before any load, the budget and the model held across admissions, a second
  model freeing the first, two admitted at once taking turns, models the
  runtime refuses, a model with a projector fitted on its weights, and one
  trained under the smallest rung
- `load_tests.rs` - a load's progress on the watch, and a shutdown stopping
  a load under way or freeing the model of one it lands on as it ends
- `lifecycle_tests.rs` - a runtime shut down, stops, a reply's `Generating`,
  a load that fails, a load that panics, and a model swapped out mid-reply

<!-- module-docs:end -->
