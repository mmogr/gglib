# ConversationListPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConversationListPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConversationListPanel-complexity.json)

<!-- module-docs:start -->

The chat page's left side: a narrow rail (new chat, search, the list's fold, the machine that answers) and the conversation list beside the notebook, with search filtering, relative timestamps and per-item delete. The list is visible by default; `LIST_POLICY` in `useListFold.ts` is the one line that makes it rail-only or always visible.

## Key Files

| File | Role |
|------|------|
| `ConversationListPanel.tsx` | Searchable list; active highlighting; relative time via `Intl.RelativeTimeFormat` |
| `ConversationListSkeleton.tsx` | Animated skeleton placeholder during initial load |
| `ConversationRail.tsx` | The 72 px rail: new chat, search, the list button, and the machine and how it is reached |
| `ConversationMarks.tsx` | A row's Running and New marks, in words, and the list button's name with their counts |
| `useConversationActivity.ts` | Running (a live agent run, from `GET /api/runs`) and New (a reply that ended while another conversation was shown); New marks are kept in this browser as conversation id → end time, read before each write and followed across tabs, and dropped for conversations no longer listed |
| `useListFold.ts` | Whether the list shows: the policy, the remembered choice, and folding by itself in a narrow window |

Timestamps use relative format for recent conversations ("2 minutes ago") and switch to absolute date strings for entries older than 7 days.

<!-- module-docs:end -->
