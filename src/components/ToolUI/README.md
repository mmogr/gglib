# ToolUI

<!-- module-docs:start -->

A tool call's result as a reply's fold shows it, under the call's row (`ChatMessagesPanel/components/ToolCallRow`): the renderer the registry holds for the tool, or the generic one, which draws JSON, Markdown or a sortable table by the result's shape (CH-12: every MCP result goes through it), and the text Copy puts on the clipboard.

## Key Files

| File | Role |
|------|------|
| `SortableTable.tsx` | Click-to-sort table with auto-inferred columns; capped at 200 rows; lexicographic and numeric comparison |
| `ToolResultDisplay.tsx` | Dispatches to `registry.getRenderer(toolName)` or falls back to `FallbackRenderer`, also when the renderer throws; `resultText`, the result as JSON to copy |

<!-- module-docs:end -->
