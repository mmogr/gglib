# proxy

<!-- module-docs:start -->

Display components for the live proxy numbers, rendered by the `ProxyDashboardModal` (the endpoint place's Live numbers). The tray popover (`tray/`) no longer uses them: it says the endpoint, what is loaded and the devices in the shared `components/status` words.

Everything here is presentational — the dashboard snapshot arrives as a prop rather than being subscribed to internally; each surface takes it from `services/dashboardRegistry`, the one stream per endpoint, and renders identical output.

## Key Files

| File | Role |
|------|------|
| `ConnectionRow.tsx` | One in-flight request: model name, phase, and a prompt-progress bar while the prompt is being processed |
| `SlotCard.tsx` | One llama.cpp inference slot as a context-usage donut and its generation rate |
| `ProxyMetricsGrid.tsx` | `ActiveConnectionsSection`, `InferenceSlotsSection` and `ModelSignalsSection`, which the modal renders with its cache panels between them |
| `ModelSignalsCard.tsx` | One model's per-model signals, under the rules `gglib proxy dashboard` prints them by: only what fired, repairs as a ratio, trips by detector, agent-path trips against their own decisions, the repeat counters under "Observed" |
| `RequestThroughput.tsx` | Requests per second as a `Readout` and a `Sparkline`, over `useMetricHistory`'s rate mode |

Cache reporting lives in `ProxyCachePanel` (one level up) rather than here — it is consumed only by the dashboard modal, which has room for it.

<!-- module-docs:end -->
