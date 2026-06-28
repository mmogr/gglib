# useChatPersistence

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-hooks-useChatPersistence-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-hooks-useChatPersistence-complexity.json)

<!-- module-docs:start -->

Bidirectional bridge between the `@assistant-ui/react` streaming runtime and the backend database. Hydrates messages from the DB when a conversation is selected, and persists new or changed messages with debouncing and deduplication to prevent spurious writes during streaming.

## Architecture

```
Conversation selected
    ▼
Effect 1 (Hydrate)
    loadMessages(conversationId) → buildLoadedMessage() per row
    foldToolMessages() ← merges tool-role rows into assistant contentParts
    runtime.messages = hydrated[]

Streaming / editing
    ▼
Effect 2 (Persist)
    detect new / changed messages (digest comparison)
    debounce 400ms
    buildSaveMetadata() ← extracts structured parts, reasoning, timing
    saveMessage(row) or deleteMessage(id)
```

## Key Files

| File | Role |
|------|------|
| `index.ts` | Main hook; hydration + persistence effects; debounce timer management |
| `buildLoadedMessage.ts` | DB row → `ThreadMessageLike`; reconstructs content parts; folds tool rows |
| `buildSaveMetadata.ts` | `ThreadMessage` → DB metadata; extracts tool-calls, reasoning, thinking duration |

## Caching Strategy

- `persistedByMessageId` — tracks which messages are in the DB
- `lastDigestByMessageId` — detects content changes since last save
- Per-message debounce timers are cleaned up on conversation switch to prevent cross-conversation writes

<!-- module-docs:end -->
