# Branching

<!-- module-docs:start -->

When a change to a saved chat branches it, and what the options are at each
of its branch points ([ADR 0017](../../../../../docs/adr/0017-history-is-never-rewritten.md)).

A saved reply is never discarded or altered. A change that would do either
copies the chat, as far as the point it changes, into a new chat of the
same family, makes the change there, and leaves the first chat as it was:
an edit of a question asked before the last, an edit of any reply, a
regenerate. Only the last question, while nothing answers it, is edited in
place. A branch is also made on its own, from any turn, to carry on from
there.

Every copy remembers the message it copies, so two chats of a family that
hold the same message at a position hold the same chat up to there. The
turns they go on with after it are the options at that branch point, and
choosing one opens the chat that holds it.

| Item | What it decides |
|---|---|
| [`units()`] | A chat's messages as turns: a question, or a reply |
| [`plan()`] | What a [`ChatChange`] writes: a [`Plan`], or why it is [`Refused`] |
| [`answerable()`] | Whether a chat ends in a question with no reply |
| [`points()`] | The [`BranchPoint`]s along a chat, from its family's [`LineChat`]s |
| [`preview()`] | The line an option is shown by |

It is pure: it reads messages and returns values; the chat history service
reads and writes what it decides, and every surface that changes a chat
goes through that service. ggchat keeps its own chats on the phone and
mirrors these rules in Swift; `contracts/chats/branching.json`, recorded
here, is the cases both replay.

It is **not** responsible for storing anything, for answering a question,
or for how a surface draws a branch point.

<!-- module-docs:end -->
