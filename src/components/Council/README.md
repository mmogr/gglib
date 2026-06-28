# Council

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Council-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-Council-complexity.json)

<!-- module-docs:start -->

Composable visualization and interaction components for the multi-agent orchestrator (Council) feature: collapsible casting sheets, DAG-tree execution graphs, compact run summary cards, and the full orchestrator run lifecycle.

## Key Files

| File | Role |
|------|------|
| `CollapsibleCastingSheet.tsx` | Role icon stack header; expands to full actor-card grid with phase badges |
| `CollapsibleDagView.tsx` | Phase-count pills header; expands to indented task tree |
| `CompactRunCard.tsx` | Single-line chip (phase icon + goal snippet) for inline display in chat messages |

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `PlanEditor/` | Interactive DAG editor with undo stack; approve/reject workflow |
| `Thread/` | Full orchestrator run lifecycle (`CouncilThread` + historical replay) |

<!-- module-docs:end -->
