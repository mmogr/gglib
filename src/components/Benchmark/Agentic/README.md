# Agentic

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-Agentic-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-Agentic-complexity.json)

<!-- module-docs:start -->

Agentic-mode UI: the raw-vs-gglib A/B eval, run against a tool-calling task
suite to measure what the gglib pipeline actually contributes over talking to
llama-server directly. Validity arms (A/A drift and a three-way control) run
alongside the comparison so a reported effect can be told apart from noise.

This is the GUI counterpart of `gglib benchmark agentic`; both surfaces drive
the same endpoints and the report follows the CLI renderer's block order.

## Key Files

| File | Role |
|------|------|
| `AgenticTab.tsx` | Orchestrator — owns all SSE/run state and composes the components below. Mirrors `TuneTab`'s streaming shape: high-frequency `agentic_task_complete` events are buffered and flushed every 100 ms; coarse events apply immediately. |
| `AgenticConfigForm.tsx` | Model select, task-suite picker, context size, seeds, and the control/replicate toggles. Mirrors the server's `DEFAULT_SEEDS` so an untouched form shows what the run will actually use. |
| `AgenticLiveProgress.tsx` | Per-arm progress and a scrolling pass/fail log. Arm names are kept identical to the CLI banner's so the two surfaces agree. |
| `AgenticReport.tsx` | The finished report — identity block, axis table, efficiency table, and JSON export in the CLI's `--output` shape. |
| `AgenticReportVerdicts.tsx` | Renders the derived verdicts: sample-size warning, A/A drift, three-way control, per-seed stability. |
| `AgenticTaskDrilldown.tsx` | Per-task expansion of a completed run — no CLI equivalent. |
| `AgenticHistoryList.tsx` | Past reports for a model, via the agentic-history endpoint. |
| `verdicts.ts` | Client-side mirror of the Rust verdict methods. See below. |

## Verdict Parity

`AgenticEvalReport`'s verdicts are **methods** on the Rust type
(`gglib_core::domain::benchmark::agentic`), not serialized fields, so they do
not arrive on the wire and the GUI re-derives them in `verdicts.ts`. The two
implementations must agree exactly: the composite-gap and noise-ratio
thresholds, the boundary inclusivity on both control branches, the
zero-effect guard, and the per-seed instability predicate. Both sides compute
in f64, so there is no floating-point divergence to account for.

The unit tests pin the TypeScript against dyadic fixtures, but note what that
does and does not buy: each side is pinned to its own copy of the constants.
Changing a threshold in the Rust leaves both suites green while the CLI and
the GUI render contradictory verdicts for the same stored report. Treat the
thresholds as a shared contract and change them in both places, or add a
source-parsing guard in the style of `tests/ts/contracts/settingsBounds.test.ts`.

<!-- module-docs:end -->
