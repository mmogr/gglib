# chat

<!-- module-docs:start -->

The chat place: one conversation, the one its address names (`/chat/:id`,
`/chat/far/:id`), or an empty thread (`/chat/new`) whose first send makes
the conversation and replaces the address. There is no conversation list,
no Chat/Console tabs and no Close: the sidebar lists the chats
(`sidebar/ChatList`) and navigates, and leaving keeps the model loaded.

The routes that read the address and pick the model are `app/ChatRoute`,
`app/PairedChat` and `app/FarChatRoute`; the list the place and the sidebar
share is `services/chatRegistry`. The thread, the composer and the picture
sheet are `components/ChatMessagesPanel`, whose head is the title ▾ and the
system prompt line.

| File | Role |
|------|------|
| `ChatPlace.tsx` | The place: a conversation on this machine's model, the paired machine's model, or one of that machine's own chats (read and carried on, never changed); hands up the conversation it opens (a first send, a branch, Start over) and a delete of the open one, so the address follows |
| `useConversationActions.ts` | What the title ▾ does to the open chat: Rename, Start over (same prompt), Export, the system prompt, Delete… (asked first, shared with the sidebar's ⋯); each acts on this machine's daemon and does nothing on a far chat |

The words this place says that are its own, not the daemon's (state words
come from `components/status/words.ts`):

- the title ▾: Rename · Generate title · System prompt… · Export · Start over (same prompt)… · Delete…; Generate title's reasons: "Start a server to generate titles", "Wait for the reply to finish", "Ask the model for a title"
- the system prompt: "Custom system prompt · Edit", Reset to default, Cancel, Save
- the confirm sheets: `Delete "<title>"?` / "Its messages are deleted too. This cannot be undone."; "Start over with the same prompt?" / "This conversation is deleted, and an empty one with its title and system prompt takes its place."
- a far chat's picker line: "<machine> picks the model"; while a route decides: "Loading chat…"

<!-- module-docs:end -->
