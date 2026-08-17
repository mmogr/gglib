# ToolsPopover

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolsPopover-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolsPopover-complexity.json)

<!-- module-docs:start -->

Popover listing all registered tools with individual enable/disable checkboxes and a toggle-all control. Tool state is read from and written to the tool registry singleton, keeping the active tool set in sync with the LLM's available function calls.

## Key Files

| File | Role |
|------|------|
| `ToolsPopover.tsx` | Tool list with checkboxes; toggle-all button; tool icon and name display |
| `AgentLimitsSection.tsx` | Per-chat knobs persisted client-side (`services/agentOverrides`): tool timeout, parallel calls, observation steps, and the two reasoning controls |

The tool list is refreshed from the registry each time the popover opens, ensuring newly registered MCP tools appear immediately.

Every number row bounds its value on blur and never on keystroke. The inputs are controlled, so a rejected keystroke blanks the field: checking `min` per keystroke rejects the below-floor *prefixes* of a legal answer ("3", "30" on the way to a 30000 ms timeout) and makes the row impossible to type into. `tests/ts/components/AgentLimitsSection.test.tsx` holds that.

The reasoning rows are not `AgentConfig` fields and do not travel in `config` — see `services/agentOverrides.ts`. They are here because this is the popover for settings that apply to the chats this client sends, and a per-turn thinking level is one; the model inspector is where a *model's* template support is stated.

<!-- module-docs:end -->
