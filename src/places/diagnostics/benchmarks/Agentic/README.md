# Agentic

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
| `AgenticConfigForm.tsx` | Model select, task-suite picker, context size, seeds, the control/replicate toggles, and the proxy-pair toggle. Mirrors the server's `DEFAULT_SEEDS` so an untouched form shows what the run will actually use. |
| `AgenticLiveProgress.tsx` | Per-arm progress and a scrolling pass/fail log. Arm names are kept identical to the CLI banner's so the two surfaces agree. |
| `AgenticReport.tsx` | The finished report — identity block, axis table, efficiency table, and JSON export in the CLI's `--output` shape. |
| `AgenticReportVerdicts.tsx` | Renders the served verdicts: sample-size warning, A/A drift, three-way control, per-seed stability. |
| `AgenticProxyPair.tsx` | The proxy arm beside its raw-auto baseline, and the proxy's repair and loop-guard counts — the CLI's "through gglib-proxy" block. Renders nothing for a report without the pair. |
| `AgenticTaskDrilldown.tsx` | Per-task expansion of a completed run — no CLI equivalent. |
| `AgenticHistoryList.tsx` | Past reports for a model, via the agentic-history endpoint. |

## Verdicts

The verdicts are the daemon's: `AgenticEvalReport` carries `control_verdict`
and `effect_verdict`, and each task its `raw_passed`, `gglib_passed` and
`unstable`, judged by today's rules wherever a report is assembled or read
back (`with_verdicts` in `gglib_core::domain::benchmark::agentic`), the
same ones `gglib benchmark agentic` prints. This folder only words them.

<!-- module-docs:end -->
