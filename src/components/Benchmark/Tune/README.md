# Tune

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-Tune-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Benchmark-Tune-complexity.json)

<!-- module-docs:start -->

Tune-mode UI: sweep a model's sampling parameters against an agentic
tool-calling task suite to find the settings that make it both accurate at
tool calls and resistant to loop/stagnation guard triggers.

## Key Files

| File | Role |
|------|------|
| `TuneTab.tsx` | Orchestrator — owns all SSE/run state, composes the three components below. Reuses the compare feature's 100 ms throttled-buffer pattern for high-frequency `tune_task_complete` events. |
| `TuneConfigForm.tsx` | Model select, sampling-parameter sweep inputs, task-suite selector (built-in default vs. custom file upload), seeding toggles, pruning override, Apply-best checkbox |
| `TuneLiveProgress.tsx` | Candidate progress bar and a scrolling log of per-task pass/fail + pruning notices |
| `TuneLeaderboard.tsx` | Sortable table of completed candidates (score, tool accuracy, loop-free rate, provenance) with a per-row Apply button |

## Custom Task Suite Parity

A custom task suite is parsed **client-side**: the uploaded JSON file must be
a plain array of task definitions in the exact same shape
`gglib benchmark tune --task-suite path.json` reads from disk on the CLI (see
`crates/gglib-core/assets/tune_default_suite.json` for the canonical
example). `TuneConfigForm` parses the file into that array and wraps it as
`{ source: 'custom', tasks: [...] }` before it is ever sent to the API —
there is one shared task schema for both surfaces, not two divergent
ingestion paths.

<!-- module-docs:end -->
