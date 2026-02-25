<!-- module-docs:start -->

# Utilities

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-utils-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-utils-complexity.json)

Shared TypeScript helpers used across the React frontend.

## Files

| File | Purpose |
|------|---------|
| `cn.ts` | Tailwind class merging utility (clsx + tailwind-merge) |
| `format.ts` | Number and date formatting helpers |
| `platform.ts` | Platform detection (Tauri vs web, OS) |
| `sse.ts` | Server-Sent Events client with reconnect logic |
| `modelSearchParser.ts` | Parse HuggingFace search queries and filters |
| `thinkingParser.ts` | Parse `<think>` blocks from reasoning model output |
| `stripThinkingBlocks.ts` | Remove thinking blocks for clean display |
| `batchWithinWindow.ts` | Batch rapid events within a time window |
| `messages/` | Chat message transformation helpers |

For Rust-side utilities (paths, config, process management), see [gglib-core](../../crates/gglib-core/README.md) and [gglib-runtime](../../crates/gglib-runtime/README.md).

<!-- module-docs:end -->
