# ToolExecutionProgress

<!-- module-docs:start -->

Real-time parallel tool execution status display embedded within a streaming assistant message. Derives one row per tool call from the message content tree, showing tool name, status icon (running/complete/error), elapsed duration, and a compact error summary on failure. Under the row of a tool still running: how far it has got, when it says, and the picture it is making.

## Key Files

| File | Role |
|------|------|
| `ToolExecutionProgress.tsx` | Reads `useMessage` content parts; maps to `AugmentedToolCallPart[]`; renders status rows, and under a running one its progress and the frame its caller holds for it (`previews`, by tool call) |
| `ToolRunProgress.tsx` | A running tool's `tool_progress` as words (queued and its place in line, loading, sampling n of N, decoding, finishing) and a step bar, and the run's latest preview frame for the call, about 128 px a side, drawn at twice that and scaled up smoothly |

The component stays mounted as a collapsed accordion after all tools settle so users can still review which tools ran. Tool names are resolved through the registry for display-friendly formatting.

<!-- module-docs:end -->
