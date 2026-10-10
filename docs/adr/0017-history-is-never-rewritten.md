# ADR 0017 — History is never rewritten: a change that would rewrite a chat branches it into a new identical chat

- **Status:** Accepted
- **Date:** 2026-10-09
- **Depends on:** [ADR 0013](0013-the-target-is-a-value.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** none

## Context

A chat could be changed in one way only, and that way destroyed what it
changed.

- **An edit and a regenerate delete.** The page's edit names the edited
  message and its regenerate names the question, as `replace_from` on an
  agent run (`src/hooks/useGglibRuntime/useGglibRuntime.ts`). Once the run
  is accepted, `SqliteChatHistoryRepository::replace_from`
  (`crates/gglib-db/src/repositories/sqlite_chat_history_repository.rs`)
  deletes that message and every later one before the new question is
  saved. An edit ten turns back loses ten turns, and a regenerate loses the
  reply it replaces, including when the new one is worse.
- **Only the page can change a chat at all.** `gglib chat` has no edit
  (`crates/gglib-cli/src/handlers/agent_chat/repl_line.rs`). A paired device
  cannot send `replace_from`: `HubTurn` refuses every key it does not name
  (`crates/gglib-core/src/domain/hub_chats.rs`), so ggchat and the page's
  far chats have none either. ggchat's own chats have none.
- **No surface can show an alternative.** A chat is one list of messages,
  and nothing records that two chats, or two replies, share a beginning.

## Decision

1. **A saved reply is never discarded or altered.** A change that would do
   either copies the chat, as far as the point it changes, into a new chat
   with the same title and settings, makes the change there, and leaves the
   chat it was made on as it was. That covers an edit of any question but
   the last, an edit of any reply, and a regenerate of any reply. A branch
   is also made on its own, from any turn, to carry on from there.
2. **The one change made in place is an edit of the chat's last question
   while nothing answers it.** Nothing is lost by it. While a reply to the
   chat is being written, that edit branches too, so an edit never moves a
   question out from under the reply answering it.
3. **Every chat stays a list.** A copied message remembers the message it
   copies, and a branch remembers the chat it was made from and the chat its
   family started with. Every reader of a chat reads it as before.
4. **The options at a branch point are the different turns the family holds
   there.** Two chats of a family that hold the same message at a position
   hold the same chat up to there; the turns they go on with are the
   options. Choosing one opens the chat that holds it: switching writes
   nothing, so no surface keeps a "current branch" that another must agree
   with.
5. **The rules are written once per language.**
   `crates/gglib-core/src/domain/branching` decides what a change writes and
   what the branch points are, and the chat history service is its only
   caller, for the daemon, the hub and the CLI alike. ggchat keeps its own
   chats and mirrors the rules in Swift; `contracts/chats/branching.json`,
   recorded by the Rust tests, is the cases both replay.
6. **Changing and answering are separate.** A change is a write that says
   whether the chat it leaves is then to be answered; answering is a run,
   as a turn is, told to answer the question already saved instead of
   saving a new one. Retry is that run on its own.

## Consequences

- Nothing a chat said is lost to an edit, on any surface.
- A chat list holds a chat per branch. Each regenerate is a new chat, so a
  family that is regenerated often is many rows, marked as branches.
- A branch copies its messages' text, and links to its images, which are
  stored once by their hash. A long chat branched often takes that much
  more room.
- A device that may add a turn may also branch a chat: a branch adds a chat
  and never takes anything away. A device still deletes nothing.

## Kill criteria

- **The list is buried in branches.** Read per family:
  `SELECT COALESCE(lineage_id, id) AS family, COUNT(*) FROM
  chat_conversations GROUP BY family ORDER BY 2 DESC` on a database in real
  use. If the largest families run past 20 chats and the list is used to
  find chats rather than search, show a family as one row with its branches
  under it; the family is already recorded, so that is a change of view, not
  of model.

## Out of scope

- Deleting: a delete still removes a message and everything after it, in
  the one chat it is made in, on this machine's page only.
- Continuing a reply that stopped part-way, which only ggchat's own chats
  do and which still only ever adds to the reply.
- Merging two branches back into one.
