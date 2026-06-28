# components

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-Council-components-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-Council-components-complexity.json)

<!-- module-docs:start -->

Composable UI components for task-graph visualisation, live debate streaming, human-in-the-loop approval, and conversational graph steering within the Council page.

## Key Files

| File | Role |
|------|------|
| `CastingSheet.tsx` | Actor-card grid; maps all leaf nodes to role icons and phase badges; flattens nested teams recursively |
| `DagView.tsx` | Indented-tree task graph; collapsible teams; topological sort; `sessionStorage`-persisted expansion state |
| `DebateNodeBody.tsx` | Live debate stream renderer: per-agent coloured text, round grouping, judge assessment, stance outcome badges |
| `HitlApprovalModal.tsx` | Three-variant approval modal (plan/node/tool/spawn-team); cost estimate; integrates `SteeringPanel` |
| `NodePanel.tsx` | Collapsible per-task panel: goal, tool allowlist, streaming text, tool calls, output, status, quick-action steering |
| `SteeringPanel.tsx` | Conversational graph steering; calls `POST /api/council/steer`; renders diff preview (green/red/amber) |

<!-- module-docs:end -->
