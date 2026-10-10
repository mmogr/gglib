# chats

<!-- module-docs:start -->

`/v1/chats`: a paired device's view of the hub's chats. `GET /v1/chats`
lists them newest first, each with the run whose reply to it is not yet
saved, and each branch with the chat it was made from (`branch_of`);
`GET /v1/chats/{id}` opens one with every row and the metadata the hub
saved, with the chat's settings, which say `"thinking": "off"` when a turn
switched its thinking off, and with what the hub says of its branches (ADR
0016): the points where its family parts (`points`) and whether it ends in a
question nothing answers (`answerable`). Served through the `HubChatsPort`
the proxy was started with, and answered `503 chats_unavailable` without one.

`POST /v1/chats/{id}/changes` edits, regenerates or branches a chat, with
the body the hub's page sends its own daemon, and the hub's branching rules
decide: a change that would rewrite a saved reply copies the chat into a new
branch. The answer is the chat to show, whether it is new, and whether it is
now to be answered, which the device does with a turn that says
`answer_saved`. A change the rules refuse is answered by its code
(`unchanged`, `not_a_reply`, `nothing_to_answer`, `message_not_found`,
`invalid_request`), a body that is no change `400 invalid_request`.

`/v1/attachments` is the images those chats' turns carry. A device sends
one as the raw body of `POST /v1/attachments`, at most 8 MiB, and is
answered its id, type, size and estimated tokens; a turn then names it by
that id. `GET /v1/attachments/{id}` answers the bytes as they were sent, with
`no-store`: a device keeps none of this machine's chats. An id that is not stored is `404
attachment_not_found`. Both go through the same port, behind the same guard.
An image a tool made is read there too, by the id its tool row lists.

Pairing is the grant, and `gglib remote forget` takes it away with the
key. Only a request the tunnel edge marked with a device's name reaches a
chat: anything else, a local client or a LAN client holding the proxy's
key, is refused `403 device_not_named`, because the hub's own page reads
its chats at `/api` and a key-holder must not read every chat. Nothing is
copied; each call reads the hub's rows.

# Module Layout

```text
chats/
  mod.rs       — the module
  attachments.rs — an image sent and one read back; no image is logged
  guard.rs     — only a named device passes
  handlers.rs  — list, open and change, with errors in the proxy's shape;
                 nothing a client sent is echoed
```

<!-- module-docs:end -->
