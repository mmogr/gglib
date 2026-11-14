# benchmarks

<!-- module-docs:start -->

Diagnostics › Benchmarks. `BenchmarkPage.tsx` is a thin shell — the mode
tabs (Perf, Compare, Tune, Agentic) and the shared run-history strip, read
when the tab opens and when a run completes — and every mode lives here as
its own module.

## Key Files

| File | Role |
|------|------|
| `BenchmarkPage.tsx` | The mode tabs, the mode open, and Recent Runs |
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
| `Agentic/` | Raw-vs-gglib A/B eval: config form, live arm progress, the report (axis table, validity verdicts, efficiency, per-task drill-down), history list; the verdicts are served on the report |

<!-- module-docs:end -->
