# ToolUI

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolUI-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolUI-complexity.json)

<!-- module-docs:start -->

Tool call rendering: collapsible cards showing execution arguments and results with status badges, a sortable data table for array results, and a dispatcher that routes to the appropriate registered renderer.

## Key Files

| File | Role |
|------|------|
| `GenericToolUI.tsx` | Collapsible card with status badge, JSON argument viewer, result display; built with `@assistant-ui/react makeAssistantToolUI` |
| `SortableTable.tsx` | Click-to-sort table with auto-inferred columns; capped at 200 rows; lexicographic and numeric comparison |
| `ToolResultDisplay.tsx` | Dispatches to `registry.getRenderer(toolName)` or falls back to `FallbackRenderer` |

<!-- module-docs:end -->
