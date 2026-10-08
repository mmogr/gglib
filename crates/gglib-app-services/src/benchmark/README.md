# benchmark

<!-- module-docs:start -->

Benchmark service: the compare, perf, tune and agentic runs.

[`BenchmarkOps`] is built in `service_graph.rs` and called only by the daemon's
HTTP handlers, in `gglib-axum`'s `handlers/benchmark/`. The CLI and the GUI
reach a run through those routes.

# Cancellation

Every run takes a `CancellationToken`. The handler wraps the run's event
stream in a [`guard::BenchmarkTaskGuard`], which cancels the token when it is
dropped: when the stream ends, or when the client goes away, which is what
Ctrl-C in the CLI does. The run stops at the next point where it checks the
token: compare and perf before each model, tune before each candidate, and the
agentic eval before each seed of each task. It then marks the run failed as
`Aborted by user` and calls `stop_current()`.

# Module Layout

```text
benchmark/
  mod.rs     — BenchmarkOps, BenchmarkDeps (public API)
  compare.rs — SSE inference loop: ModelRuntimePort orchestration +
               defensive parsing of the reply, which
               gglib_core::sse::DataFrames cuts into its chunks
  perf.rs    — llama-bench process spawning + VRAM drain logic
  mapper.rs  — raw serde_json::Value → domain type transforms
  guard.rs   — BenchmarkTaskGuard (DropCancels pattern for HTTP layer)
  agentic.rs — the raw-vs-gglib agentic eval, and its optional proxy pair
  proxy_arm.rs — the in-process gglib-proxy the eval's proxy arm talks to,
                 with the stand-in ports it is handed in proxy_arm_ports.rs
```

# VRAM Contention Prevention

`BenchmarkDeps::runtime` is the **same** [`ModelRuntimePort`] instance
shared with `ProxyOps` at the composition root (created once in
`bootstrap.rs`).  Both operations go through the same
[`ProcessManager`] and its admission queue, which guarantees every
llama-server on the machine lives in the same bounded resident set.
`run_perf()` additionally calls `stop_current()` before
spawning `llama-bench` so that the GPU is free when the binary loads the
model directly.

The agentic eval's proxy arm starts a second proxy, in-process, and keeps
the same guarantee by giving it a runtime port that cannot launch anything:
it answers every admission with the model the eval already holds, refuses
any other, and refuses to stop the held one.

# Defensive Parsing Contract

All JSON-to-domain-type transforms are delegated to [`mapper`].  Timing
fields are `Option<f64>`; a missing or malformed `timings` object in
llama-server's SSE response produces `None` for every timing field — never
a panic or hard error.

[`ModelRuntimePort`]: gglib_core::ports::ModelRuntimePort
[`ProcessManager`]: gglib_runtime::process::ProcessManager

<!-- module-docs:end -->
