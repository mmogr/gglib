# benchmark

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-complexity.json)

<!-- module-docs:start -->

Benchmark service — shared between CLI and web adapters.

# Module Layout

```text
benchmark/
  mod.rs     — BenchmarkOps, BenchmarkDeps (public API)
  compare.rs — SSE inference loop: ModelRuntimePort orchestration +
               defensive stream parsing
  perf.rs    — llama-bench process spawning + VRAM drain logic
  mapper.rs  — raw serde_json::Value → domain type transforms
  guard.rs   — BenchmarkTaskGuard (DropCancels pattern for HTTP layer)
```

# VRAM Contention Prevention

`BenchmarkDeps::runtime` is the **same** [`ModelRuntimePort`] instance
shared with `ProxyOps` at the composition root (created once in
`bootstrap.rs`).  Both operations go through the same `SingleSwap`
[`ProcessManager`], which guarantees only one llama-server runs at a time
system-wide.  `run_perf()` additionally calls `stop_current()` before
spawning `llama-bench` so that the GPU is free when the binary loads the
model directly.

# Defensive Parsing Contract

All JSON-to-domain-type transforms are delegated to [`mapper`].  Timing
fields are `Option<f64>`; a missing or malformed `timings` object in
llama-server's SSE response produces `None` for every timing field — never
a panic or hard error.

[`ModelRuntimePort`]: gglib_core::ports::ModelRuntimePort
[`ProcessManager`]: gglib_runtime::process::ProcessManager

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`compare.rs`](compare.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-compare-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-compare-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-compare-coverage.json) |
| [`guard.rs`](guard.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-guard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-guard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-guard-coverage.json) |
| [`mapper.rs`](mapper.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-mapper-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-mapper-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-mapper-coverage.json) |
| [`perf.rs`](perf.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-perf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-perf-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-benchmark-perf-coverage.json) |
<!-- module-table:end -->

</details>
