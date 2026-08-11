# Benchmark

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-complexity.json)

<!-- module-docs:start -->

Benchmark feature components. `BenchmarkPage.tsx` (in `src/pages/`) is a thin
shell — header, mode tabs, and the shared run-history strip — and every mode
lives here as its own module.

## Key Files

| File | Role |
|------|------|
| `PerfCompareTab.tsx` | Perf/compare config aside + live results column |
| `usePerfCompareRun.ts` | Run state + SSE streaming (100 ms `model_text_delta` throttle) |
| `PerfCompareResultCard.tsx` | One model's live/complete/failed result card |
| `ModelMultiSelect.tsx` | The config panel's model checkbox list |
| `SuitePicker.tsx` | Default-vs-custom task-suite selector with client-side JSON parse |
| `RunHistoryTable.tsx` | "Recent Runs" strip, shared by all modes |
| `format.ts` | Shared tps/ms/date/delta/factor formatting |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `Tune/` | Config form, live progress, and leaderboard components for tune-mode runs |
| `Agentic/` | Raw-vs-gglib A/B eval: config form, live arm progress, the report (axis table, validity verdicts, efficiency, per-task drill-down), history list, and the client-side verdict derivations mirroring the Rust thresholds |

<!-- module-docs:end -->
