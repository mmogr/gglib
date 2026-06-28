# ToolsPopover

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolsPopover-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolsPopover-complexity.json)

<!-- module-docs:start -->

Popover listing all registered tools with individual enable/disable checkboxes and a toggle-all control. Tool state is read from and written to the tool registry singleton, keeping the active tool set in sync with the LLM's available function calls.

## Key Files

| File | Role |
|------|------|
| `ToolsPopover.tsx` | Tool list with checkboxes; toggle-all button; tool icon and name display |

The tool list is refreshed from the registry each time the popover opens, ensuring newly registered MCP tools appear immediately.

<!-- module-docs:end -->
