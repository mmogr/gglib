# diagnostics

<!-- module-docs:start -->

The diagnostics place, at `/diagnostics/:tab`: a tablist of Live, Logs,
Benchmarks and System, the open tab named by the address. `/diagnostics`
and a segment that names no tab go to Live. The sidebar foot's Diagnostics
row opens it on Live and is marked current on every tab; the endpoint
place's ⋯ Live numbers, Activity's Live numbers › and Recent's All › open
Live; a model's ⋯ Benchmark… opens Benchmarks with `?models=` picking it.

- **Live** (`live/`): the endpoint's live numbers from the one dashboard
  stream, each section only when it has something, the rest folded into one
  line.
- **Logs**: a loaded model's llama-server output, picked from the models
  loaded now, with its port, uptime, context, each llama.cpp slot's KV use
  (the primary model only, slot by slot as the CLI prints them), writing
  rate and active requests beside it, each only when the daemon has it. A
  pick that unloads is forgotten. With nothing loaded, one sentence.
- **Benchmarks** (`benchmarks/`): Perf, Compare, Tune and Agentic as
  segmented tabs, Recent Runs under them, read on opening and after a run.
  Its code loads when the tab opens, and once opened it stays mounted,
  hidden under another tab, so a run in flight survives a look elsewhere.
- **System**: `gglib config check-deps`, `paths` and `fast-downloads
  status`: dependencies, acceleration, paths, the download accelerator, and
  Re-check.

Facts with no route are not drawn: the requests per second and each slot's
generation rate (the dashboard counts requests but rates none), a model's
prompt and generated token totals (only llama-server's own `/metrics` had
them), and the `-v` debug log's path (the daemon's paths do not name its
logs folder).

Words with no Rust twin: the tab names "Live", "Logs", "Benchmarks",
"System"; "Diagnostics"; Live's section names "Requests", "Loaded and
queue", "Launch decisions", "Prompt cache", "Agent cache", "Sampling
readback", "Per-model signals", "Tune activity", "Slots", "Loop-guard
trips"; "<n> since the endpoint turned on", "Clear", "Prompt cache cleared"
(when the proxy's answer has no message), "Nothing yet: <names>.", "In
flight", "Recent requests", "Slot <n> context",
"The endpoint is off. Its live numbers show while it is on.", "Reading the
endpoint's numbers…", "Slot <n> · active", "<used> of <size> tokens · <n>%",
"context unknown"; Logs' "Facts", "Model", "Port", "Uptime", "Context", "KV use · slot <n>",
"Writing", "Active requests", "<n> tokens", "Nothing is loaded. A model's
log shows here while it is loaded."; "Loading".

| File | Role |
|------|------|
| `DiagnosticsPlace.tsx` | The route: the tablist, the open tab by the address, Benchmarks lazy with `?models=`, System's column |
| `LogsTab.tsx` | Logs: the loaded models' picker, the facts column, and the picked model's log |
| `logFacts.ts` | The facts beside a log, each once and only when the daemon has it |
| `DiagnosticsPanel.tsx` | System: `gglib config check-deps`, `paths` and `fast-downloads status` as one panel, with Re-check |
| `useDiagnostics.ts` | System's one read, Re-check, and the accelerator's toggle |
| `live/` | The Live tab and the dashboard's panels: its own `README.md` |
| `benchmarks/` | The Benchmarks tab: its own `README.md` |

<!-- module-docs:end -->
