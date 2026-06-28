# ConsoleInfoPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConsoleInfoPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConsoleInfoPanel-complexity.json)

<!-- module-docs:start -->

Left panel in the console view showing the served model's identity, real-time inference metrics (KV-cache usage, token throughput), uptime clock, and a stop server button. Polls the llama-server `/metrics` endpoint and subscribes to server lifecycle events via `useServerState`.

## Key Files

| File | Role |
|------|------|
| `ConsoleInfoPanel.tsx` | Metrics polling, uptime tick, stop-server action; syncs display with `serverRegistry` state |

## Props

| Prop | Role |
|------|------|
| `modelId` | Identity of the running server |
| `serverPort` | Port for metrics polling |
| `contextLength` | Max context window for KV-cache % calculation |
| `startTime` | Server start time for uptime display |
| `onStopServer` | Callback wired to stop button |

Polling automatically pauses when the server stops and resumes on the next `server:started` event.

<!-- module-docs:end -->
