# useChatPersistence

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-hooks-useChatPersistence-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-hooks-useChatPersistence-complexity.json)

<!-- module-docs:start -->

Turning a conversation's saved rows into the messages the thread shows. The daemon writes every turn: the user's message when a run starts and the reply when it ends (`PUT /api/runs/{id}?kind=agent`). The page saves no turn; it loads rows, deletes them (a delete, an edit, a regenerate), and renames conversations.

## Key Files

| File | Role |
|------|------|
| `buildThreadMessages.ts` | A conversation's rows → thread messages, system prompt first; shared by opening a conversation, a run's end and a delete |
| `buildLoadedMessage.ts` | One row → `ThreadMessageLike`: content parts and reasoning restored, tool rows folded into the assistant row that called them, an unfinished reply (`metadata.incomplete`) marked as a cancelled one |

Every loaded message keeps its row's id in its runtime id (`db-<id>`), which is how a delete, an edit and a regenerate find the row.

<!-- module-docs:end -->
