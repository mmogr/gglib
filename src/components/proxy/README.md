# proxy

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-proxy-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-proxy-complexity.json)

<!-- module-docs:start -->

Display components shared by every surface that shows proxy state: the `ProxyControl` dropdown in the header, the `ProxyDashboardModal`, and the system tray popover (`pages/TrayPanel`).

The tray popover shows the same information as the in-app dashboard, so without a shared home the two would have been the same markup maintained twice and drifting apart on the first styling change. Everything here is presentational — the dashboard snapshot arrives as a prop rather than being subscribed to internally, so each surface owns its own `useProxyDashboard` subscription while rendering identical output.

## Key Files

| File | Role |
|------|------|
| `ProxyStatusPill.tsx` | Running/stopped badge. Stopped uses the neutral offline colour, not danger red — idle is not a failure |
| `EndpointCopyBar.tsx` | The endpoint URL plus a copy button; exports `proxyEndpointUrl` for callers that need the string alone. Copy confirmation is delegated via `onCopied`, since the tray window has no toast host |
| `ConnectionRow.tsx` | One in-flight request: model name, phase, and a prompt-progress bar while the prompt is being processed |
| `SlotCard.tsx` | One llama.cpp inference slot as a context-usage donut; `size` shrinks it for the popover |
| `ProxyMetricsGrid.tsx` | `ActiveConnectionsSection` and `InferenceSlotsSection`, plus `ProxyMetricsGrid` composing both. The modal uses the sections individually because its cache panels sit between them; the popover uses the pair. `compact` renders at popover scale |
| `ProxyToggleButton.tsx` | Start/stop control, so the destructive styling always tracks the destructive action |

Cache reporting lives in `ProxyCachePanel` (one level up) rather than here — it is consumed only by the dashboard modal, which has room for it.

<!-- module-docs:end -->
