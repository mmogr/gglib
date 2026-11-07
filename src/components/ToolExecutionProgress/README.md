# ToolExecutionProgress

<!-- module-docs:start -->

A reply's tool calls as its fold lists them, and how far a running one has got. The rows themselves are drawn by `ChatMessagesPanel/components/ToolCallRow`.

## Key Files

| File | Role |
|------|------|
| `toolCallState.ts` | One row per tool-call part of a reply: running, done or failed (from `isError`), the name it is shown by (the daemon's `displayName`, or the registry's original name formatted), the MCP server, its time, a failure's first 80 characters, a running tool's progress, its arguments and result; read the same from a live part and a reloaded one |
| `ToolRunProgress.tsx` | A running tool's `tool_progress` as words (queued and its place in line, loading, sampling n of N, decoding, finishing) and a step bar, and the run's latest preview frame for the call, about 128 px a side, drawn at twice that and scaled up smoothly |

<!-- module-docs:end -->
