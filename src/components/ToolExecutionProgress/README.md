# ToolExecutionProgress

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolExecutionProgress-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ToolExecutionProgress-complexity.json)

<!-- module-docs:start -->

Real-time parallel tool execution status display embedded within a streaming assistant message. Derives one row per tool call from the message content tree, showing tool name, status icon (running/complete/error), elapsed duration, and a compact error summary on failure.

## Key Files

| File | Role |
|------|------|
| `ToolExecutionProgress.tsx` | Reads `useMessage` content parts; maps to `AugmentedToolCallPart[]`; renders status rows |

The component stays mounted as a collapsed accordion after all tools settle so users can still review which tools ran. Tool names are resolved through the registry for display-friendly formatting.

<!-- module-docs:end -->
