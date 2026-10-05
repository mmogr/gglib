# chats

<!-- module-docs:start -->

`/v1/chats`: a paired device's view of the hub's chats. `GET /v1/chats`
lists them newest first, each with the run whose reply to it is not yet
saved; `GET /v1/chats/{id}` opens one with every row and the metadata the
hub saved, and with the chat's settings, which say `"thinking": "off"` when a
turn switched its thinking off. Served through the `HubChatsPort` the proxy
was started with, and answered `503 chats_unavailable` without one.

`/v1/attachments` is the images those chats' turns carry. A device sends
one as the raw body of `POST /v1/attachments`, at most 8 MiB, and is
answered its id, type, size and estimated tokens; a turn then names it by
that id. `GET /v1/attachments/{id}` answers the bytes as they were sent, with
`no-store`: a device keeps none of this machine's chats. An id that is not stored is `404
attachment_not_found`. Both go through the same port, behind the same guard.

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
  handlers.rs  — list and open, with errors in the proxy's shape; nothing a
                 client sent is echoed
```

<!-- module-docs:end -->
