# chats

<!-- module-docs:start -->

`/v1/chats`: a paired device's view of the hub's chats. `GET /v1/chats`
lists them newest first, each with the run whose reply to it is not yet
saved; `GET /v1/chats/{id}` opens one with every row and the metadata the
hub saved. Served through the `HubChatsPort` the proxy was started with,
and answered `503 chats_unavailable` without one.

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
  guard.rs     — only a named device passes
  handlers.rs  — list and open, with errors in the proxy's shape; nothing a
                 client sent is echoed
```

<!-- module-docs:end -->
