# useChatPersistence

<!-- module-docs:start -->

Turning a conversation's saved rows into the messages the thread shows. The daemon writes every turn: the user's message when a run starts and the reply when it ends (`PUT /api/runs/{id}?kind=agent`). The page saves no turn; it loads rows, deletes them (the delete button), and renames conversations. An edit or a regenerate replaces rows through the run itself (`replace_from`).

## Key Files

| File | Role |
|------|------|
| `buildThreadMessages.ts` | A conversation's rows → thread messages, system prompt first; shared by opening a conversation, a run's end and a delete |
| `buildLoadedMessage.ts` | One row → `ThreadMessageLike`: content parts and reasoning restored, tool rows folded into the assistant row that called them, an unfinished reply (`metadata.incomplete`) marked as a cancelled one, a user row's images as its attachments (by id, with their facts, no bytes) |

Every loaded message keeps its row's id in its runtime id (`db-<id>`), which is how a delete, an edit and a regenerate find the row.

<!-- module-docs:end -->
