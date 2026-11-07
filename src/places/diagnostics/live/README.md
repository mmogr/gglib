# live

<!-- module-docs:start -->

Diagnostics › Live: the endpoint's live numbers. The frame is the one
stream `services/dashboardRegistry` keeps for the endpoint, through
`places/library/useResidency`'s `useEndpointDashboard`, the target the
sidebar and the endpoint place read too, so opening Live opens no second
stream. Tune activity and the loop guard's log are the daemon's reads, made
when the tab opens.

Sections, each drawn only when it has something (`sections.ts`), in this
order: Requests (in flight, then every request the proxy keeps, newest
first, and the total since the endpoint turned on), Loaded and queue (the
residents, swaps, requests admitted, the second slot's sentence in full),
Launch decisions, Prompt cache (with Clear, which presents the revealed
key), Agent cache, Sampling readback, Per-model signals, Tune activity,
Slots (llama.cpp's slots on the primary as bars with their tokens and
percent, or the proxy's reason it reports none while a model is loaded),
Loop-guard trips (the last 30 days, the window `gglib proxy trips` reads by
default). The names of the sections with nothing fold into one 12 px line.

The panels are presentational: the frame, or the read's state, arrives as
a prop.

| File | Role |
|------|------|
| `LiveTab.tsx` | The tab: the stream, the two reads, Clear, the sections in order and the folded line |
| `sections.ts` | Whether each frame section has anything to show |
| `ProxyMetricsGrid.tsx` | `RequestsSection`, `SlotsSection` and `ModelSignalsSection`, drawn from the frame |
| `ConnectionRow.tsx` | One in-flight request: model name, phase, and a prompt-progress bar while the prompt is being processed |
| `ModelSignalsCard.tsx` | One model's per-model signals, under the rules `gglib proxy dashboard` prints them by: only what fired, repairs as a ratio, trips by detector, agent-path trips against their own decisions, the repeat counters under "Observed" |
| `ProxyAdmissionPanel.tsx` | Which models hold VRAM, what is queued behind them, the render on the GPU, why the second slot is or is not in use, requests admitted and swaps |
| `ProxyLaunchPanel.tsx` | What the running model's launch decided, and why, as the backend worded it |
| `ProxyCachePanel.tsx` | The prompt cache's warnings, measured reuse, RAM budget and disk cache; `CacheUsageRows` for either population |
| `ProxySamplingPanel.tsx` | Whether the resolved sampling reached llama-server, blind kept apart from agreement, and the build's baseline |
| `ProxyReasoningRows.tsx` | The reasoning controls gglib sent, and the client fields the trust gate dropped |
| `TuneActivityCard.tsx` | `useTuneActivity`, the tune runs' verdicts read on opening, and the feed: applies and refusals alike |
| `LoopGuardTripsPanel.tsx` | `gglib proxy trips`: per day and model, what the loop guard scanned and what it acted on |
| `useLoopGuardTrips.ts` | The trips' one read when the tab opens, and a reload |

<!-- module-docs:end -->
