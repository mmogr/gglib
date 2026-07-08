# benchmark

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-benchmark-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-benchmark-complexity.json)

<!-- module-docs:start -->

Benchmark domain types.

Two benchmark modes today (a third, tuning, is being added incrementally):
- **Compare** ([`compare`]): send the same prompt to N models sequentially;
  capture live streamed text and real-world timing data from llama-server's
  `timings` response field.
- **Perf** ([`perf`]): run `llama-bench` for raw prompt-processing (pp) and
  token-generation (tg) throughput in tokens/sec.
- **Tune** ([`tune`]): sweep sampling parameters for one model against an
  agentic tool-calling task suite, scoring each candidate for tool-call
  accuracy and loop/stagnation avoidance to find the best-scoring settings.

All timing fields are `Option<f64>` because llama-server may omit the
`timings` object (e.g. older builds, stream errors). Missing timing data
is gracefully represented as `None` — never causes a panic or parse error.

# Modules

| Module | Contents |
|--------|----------|
| [`run`] | [`BenchmarkRun`], [`BenchmarkRunType`], [`BenchmarkRunStatus`] |
| [`summary`] | [`ModelBenchmarkSummary`] — denormalised per-model aggregate |
| [`compare`] | [`CompareConfig`], [`ModelCompareResult`] |
| [`perf`] | [`PerfConfig`], [`ModelPerfResult`] |
| [`tune`] | [`TuneConfig`], task-suite schema, scoring result types |
| [`events`] | [`BenchmarkEvent`] (SSE units), [`BenchmarkModelResult`] |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`compare.rs`](compare.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-compare-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-compare-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-compare-coverage.json) |
| [`events.rs`](events.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-events-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-events-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-events-coverage.json) |
| [`perf.rs`](perf.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-perf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-perf-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-perf-coverage.json) |
| [`run.rs`](run.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-run-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-run-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-run-coverage.json) |
| [`summary.rs`](summary.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-summary-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-summary-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-benchmark-summary-coverage.json) |
| [`tune/`](tune/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tune-coverage.json) |
<!-- module-table:end -->

</details>
