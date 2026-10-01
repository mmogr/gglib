# ConsoleInfoPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConsoleInfoPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConsoleInfoPanel-complexity.json)

<!-- module-docs:start -->

Left panel in the console view showing the served model's identity, real-time inference metrics (KV-cache usage, token throughput), uptime clock, and an Unload model button, which stops the model and leaves the chat open. Polls the llama-server `/metrics` endpoint and subscribes to server lifecycle events via `useServerState`.

## Key Files

| File | Role |
|------|------|
| `ConsoleInfoPanel.tsx` | Composition root: wires hooks to sections, the unload action; syncs display with `serverRegistry` state |
| `useServerMetrics.ts` | 2s `/metrics` poll (Prometheus parse); each poll's fresh object doubles as the metric-history tick |
| `useUptime.ts` | Wall-clock-synced uptime string |
| `TelemetrySections.tsx` | Context-usage and generation-rate readouts with sparklines (`useMetricHistory`, reset per server run) |
| `StaticSections.tsx` | Server info rows and the API endpoint list |

## Props

| Prop | Role |
|------|------|
| `modelId` | Identity of the running server |
| `serverPort` | Port for metrics polling |
| `contextLength` | Max context window for KV-cache % calculation |
| `startTime` | Server start time for uptime display |
| `onUnload` | Callback wired to the Unload model button; no button without it |

Polling automatically pauses when the server stops and resumes on the next `server_started` frame.

<!-- module-docs:end -->
