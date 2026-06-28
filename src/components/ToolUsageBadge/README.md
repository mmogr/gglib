# ToolUsageBadge

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolUsageBadge-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolUsageBadge-complexity.json)

<!-- module-docs:start -->

Inline badge in message bubbles summarising tool usage for an assistant turn. Shows an aggregate status icon (running/success/error/mixed) and opens a details modal listing each tool call with arguments, result, and a copy button.

## Key Files

| File | Role |
|------|------|
| `ToolUsageBadge.tsx` | Status derivation from message content parts; badge with aggregate icon |
| `ToolDetailsModal.tsx` | Per-tool expandable sections; argument display; `ToolResultDisplay` for result rendering |

Status is derived from the union of all `ToolCallPart` states: `running` if any tool is still executing, `mixed` if some succeeded and some failed, otherwise `success` or `error`.

<!-- module-docs:end -->
