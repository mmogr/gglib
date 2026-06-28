# ConversationListPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConversationListPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConversationListPanel-complexity.json)

<!-- module-docs:start -->

Left sidebar listing past conversations with search filtering, relative timestamps, per-item delete, and a new conversation shortcut. Includes a skeleton loader for the initial DB fetch.

## Key Files

| File | Role |
|------|------|
| `ConversationListPanel.tsx` | Searchable list; active highlighting; relative time via `Intl.RelativeTimeFormat` |
| `ConversationListSkeleton.tsx` | Animated skeleton placeholder during initial load |

Timestamps use relative format for recent conversations ("2 minutes ago") and switch to absolute date strings for entries older than 7 days.

<!-- module-docs:end -->
