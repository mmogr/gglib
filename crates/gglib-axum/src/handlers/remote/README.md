# remote

<!-- module-docs:start -->

Handlers for the remote tunnel ([ADR 0012](../../../../../docs/adr/0012-the-remote-tunnel.md)):
enable, disable, status on the serve side; invite, the device list and forget
for who may use it; join, disconnect, kill on the connect side. Thin: each
maps one `RemoteOps` call onto the wire.

`chats.rs` is the connect side too: the far machine's chats and runs for this
machine's chat page (`/api/remote/chats`, `/chats/{id}`,
`/chats/{id}/turns/{run_id}`, `/runs`, `/runs/{run_id}/events`,
`/runs/{run_id}/cancel`), each forwarded through the tunnel with the stored
key by the `gglib_app_services::FarProxy` that `RemoteOps::far` builds. A
turn sends only `{content}`; the far machine adds the chat's history itself.
Bodies pass through and events stream through as they come; nothing is kept.
A far refusal keeps its status and code, but a refused key is a `409`, since
a `401` would have the page ask for this daemon's own key.

`models.rs` is the far machine's models, through the same `FarProxy`:
`GET /api/remote/models` answers `PairedModels` (the machine, what may be
done to a model there, and every entry its `/v1/models` lists, profile
variants included), `GET /api/remote/models/{model}` one model as a
`ModelLookup`, and `POST /api/remote/models/{model}/load` a `LoadResponse`.
`{model}` is an identifier that machine resolves, sent as one encoded path
segment. These are read rather than passed through, so the CLI and the page
parse far rows in one place; a refusal is handed on as the chats' are, and a
far build too old to publish model ids is a `409` that asks for an update.
Every far route is a `409` that says `gglib remote join` while this machine is
joined to nothing.

Two decisions live here rather than in `RemoteOps`:

- **Enable is not idempotent.** A second `enable` while the tunnel is up is a
  `409`. Answering it would mean minting a second pairing code for a live
  session or re-reading the first, and the response is the only place the
  ticket and the code are ever shown — a `GET` that could return them would
  make them retrievable by anything that can call `GET`.
- **Disable is.** A tunnel that is already down is the outcome asked for, so
  the handler answers with the status rather than a conflict.

- **Join is not idempotent either**, turned around: a second `join`
  while connected is a `409` rather than a silent reuse, because the second
  call may name a different machine. **Disconnect is.**
- **Kill asks for the word.** `{"confirm":"shutdown"}` or a `400` that changes
  nothing — the same contract as the proxy route it forwards to, kept at this
  hop too so a GUI cannot reach the one-way door with an empty `POST`.

- **Forgetting a device that is already gone is a `200`, not a `404`.** The
  outcome asked for is that the machine holds no key under that name, and it
  does not. The body says `{"forgotten": false}` so a surface that wants to
  tell the difference can, and one that only wants the device gone need not.

`invite` has no shape of its own: `RemoteOps::invite` answers with the same
`Enabled` an `enable --invite` does — the ticket belongs in it, and a pairing
view needs one — so the route reuses the enable response and a client needs
no second shape to decode.

The shapes are not here. They are `gglib-app-services`' (`remote/wire.rs` and
`remote/wire_exchange.rs`): the daemon reads the enable and join bodies and
answers with the rest, the CLI sends those bodies and reads the answers, and
`ts-rs` exports them for the Remote panel. Only two bodies are this crate's:
the kill body beside its handler in `join`, and the load body beside its
handler in `models`.

The status is the response anything on this machine can ask for twice, so
what it leaves out is as much the contract as what it carries: the ticket's
fingerprint and never the ticket, peers by fingerprint, the connect side's
port and path, what settings remember of the last pairing (again by
fingerprint, and by the name the paired machine gave), the counters the
tunnel's owner keeps, and device rows with no field a key could live in. Each
row carries the daemon's own description of it, for a surface to print. The
paired machine's fingerprint is its identity, which a surface compares;
what a surface shows it as is that name, and the join answer carries the same
name for the machine joined and for the pairing it replaced.

<!-- module-docs:end -->
