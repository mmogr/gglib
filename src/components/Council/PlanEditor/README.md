# PlanEditor

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Council-PlanEditor-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Council-PlanEditor-complexity.json)

<!-- module-docs:start -->

Interactive visual editor for task graphs. Allows users to inspect and modify orchestrator plans before approving execution — editing node goals, tool allowlists, and dependencies, with undo/redo and an approve/reject decision path.

## Key Files

| File | Role |
|------|------|
| `PlanEditor.tsx` | Root editor: node selection, property panel, approval controls |
| `usePlanEditor.ts` | Structural-sharing undo stack; edit tracking; diff detection |
| `DebateRosterEditor.tsx` | Sub-component for editing debate team composition |

The editor treats `TaskGraph` as immutable — each edit produces a new snapshot stored in the undo stack. `usePlanEditor` tracks whether the current graph differs from the original to enable the "approve with edits" path.

<!-- module-docs:end -->
