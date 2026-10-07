# ToolsPopover

<!-- module-docs:start -->

Popover listing all registered tools with individual enable/disable checkboxes and a toggle-all control. Tool state is read from and written to the tool registry singleton, keeping the active tool set in sync with the LLM's available function calls.

## Key Files

| File | Role |
|------|------|
| `ToolsPopover.tsx` | Tool list with checkboxes; toggle-all button; tool icon and name display; asks once, when first opened, whether the chat's model reads a reasoning effort |
| `AgentLimitsSection.tsx` | Device-wide knobs persisted client-side (`services/agentOverrides`): tool timeout, parallel calls, observation steps, and the two reasoning controls, the effort one drawn by `ReasoningEffortField` for what is known of the model's template |

The tool list is refreshed from the registry each time the popover opens, ensuring newly registered MCP tools appear immediately.

The popout lines up with the button's right edge by default; the composer's button sits in the notebook's left margin, so it passes `align="left"` and the popout grows rightwards over the page instead of off it. Its height is capped at 70vh and scrolls, so a short window cannot clip its top.

Every number row bounds its value on blur and never on keystroke. The inputs are controlled, so a rejected keystroke blanks the field: checking `min` per keystroke rejects the below-floor *prefixes* of a legal answer ("3", "30" on the way to a 30000 ms timeout) and makes the row impossible to type into. `tests/ts/components/AgentLimitsSection.test.tsx` holds that.

The reasoning rows are not `AgentConfig` fields and do not travel in `config` — see `services/agentOverrides.ts`. They are here because this is the popover for settings that apply to the chats this client sends, and a per-turn thinking level is one; the model inspector is where a *model's* template support is stated.

The effort row is `InferenceParametersForm/ReasoningEffortField.tsx`, the settings surfaces' own three-state field, so the popover says the same thing they do. The popover is given the chat's model by its id here (`modelId`) and reads that model's `reasoningEffortSupport` from `getModelDetail` the first time it opens, never at mount: `no` replaces the dropdown with the field's note, `unknown` and `yes` keep it, and a chat with no model of this machine (a paired machine's model, a far chat) or an answer that has not come gets the dropdown with the caption for no model in scope. A level stored before stays stored and is still sent under `no`; the daemon removes it. The budget row is under it whatever the answer.

Both reasoning controls are one value per browser, and one that is set is sent with every local run. What one chat does is the composer's Thinking switch (`hooks/useThinkingSwitch.ts`), which neither reads nor writes them: a chat switched off there runs with a budget of 0 whatever the budget here says, because the daemon lets the chat's choice win, and the caption under the budget row says so.

<!-- module-docs:end -->
