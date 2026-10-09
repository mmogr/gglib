<!-- module-docs:start -->

# Utilities

Shared TypeScript helpers used across the React frontend.

## Files

| File | Purpose |
|------|---------|
| `cn.ts` | Tailwind class merging utility (clsx + tailwind-merge) |
| `format.ts` | Number and date formatting helpers |
| `sse.ts` | The one Server-Sent Events reader: `readSse` yields the events of a `fetch` response, and `createSSEStream` opens a stream that is not the daemon's and reads it. Reconnecting is the caller's |
| `modelSearchParser.ts` | Parse HuggingFace search queries and filters |
| `batchWithinWindow.ts` | Batch rapid events within a time window |
| `mcp.ts` | MCP server status predicates (running / unsupported / error state) |
| `samplingProvenance.ts` | Render a resolved sampling parameter and the layer that supplied it; wording mirrors `gglib model explain` |
| `errors.ts` | `isAbortError`, the predicate for the `DOMException` both `fetch()` and stream reads throw when a signal fires, and `formatError`, a thrown value as the message to show |
| `formatPerSecond.ts` | Compact per-second count with no unit; the caller supplies "tok/s", "req/s" or whatever it counts |
| `imageFamily.ts` | An image family's name as Rust's `ImageFamily::label` gives it, and a component role's name |
| `canSee.ts` | Whether a row's model reads images: a local row's `imageInput`, or a far row's `vision` capability |
| `thinks.ts` | Whether a row's model thinks, which is what a Thinking switch is offered for: a local row's `reasoning` tag in any case (never its capability bit), or a far row's `reasoning` capability |
| `contextUsage.ts` | A usage meter's whole-number percent (a half rounds up, never over 100) and its severity (warning from 70, danger from 90), so a meter's colour, figure and words agree |
| `dbTimestamp.ts` | Parse a database timestamp, reading SQLite's zone-less `YYYY-MM-DD HH:MM:SS` as the UTC it is rather than as local time |
| `messages/` | Chat message transformation helpers |

For Rust-side utilities (paths, config, process management), see [gglib-core](../../crates/gglib-core/README.md) and [gglib-runtime](../../crates/gglib-runtime/README.md).

<!-- module-docs:end -->
