# remote

<!-- module-docs:start -->

Handlers for the remote tunnel ([ADR 0012](../../../../../docs/adr/0012-the-remote-tunnel.md)):
enable, disable, status on the serve side; invite, the device list and forget
for who may use it; connect, disconnect, kill on the connect side. Thin: each
maps one `RemoteOps` call onto the wire.

Two decisions live here rather than in `RemoteOps`:

- **Enable is not idempotent.** A second `enable` while the tunnel is up is a
  `409`. Answering it would mean minting a second pairing code for a live
  session or re-reading the first, and the response is the only place the
  ticket and the code are ever shown — a `GET` that could return them would
  make them retrievable by anything that can call `GET`.
- **Disable is.** A tunnel that is already down is the outcome asked for, so
  the handler answers with the status rather than a conflict.

- **Connect is not idempotent either**, turned around: a second `connect`
  while connected is a `409` rather than a silent reuse, because the second
  call may name a different machine. **Disconnect is.**
- **Kill asks for the word.** `{"confirm":"shutdown"}` or a `400` that changes
  nothing — the same contract as the proxy route it forwards to, kept at this
  hop too so a GUI cannot reach the one-way door with an empty `POST`.

- **Forgetting a device that is already gone is a `200`, not a `404`.** The
  outcome asked for is that the machine holds no key under that name, and it
  does not. The body says `{"forgotten": false}` so a surface that wants to
  tell the difference can, and one that only wants the device gone need not.

`devices` holds its own shapes rather than putting them in `wire`, which is
fifteen lines from the file-size budget. `invite` has none: `RemoteOps::invite`
answers with the same `Enabled` an `enable --invite` does — the ticket belongs
in it, and a pairing view needs one — so the route reuses the enable response
and a client needs no second shape to decode.

`wire` holds the shapes. The status carries the ticket's fingerprint and never
the ticket; the peers by fingerprint; the connect side's port and path; what
settings remember of the last pairing, again by fingerprint; and the counters
the tunnel's owner keeps. `ts-rs` exports them for the Remote panel.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`connect.rs`](connect.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-connect-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-connect-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-connect-coverage.json) |
| [`devices.rs`](devices.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-devices-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-devices-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-devices-coverage.json) |
| [`wire.rs`](wire.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire-coverage.json) |
| [`wire_tests.rs`](wire_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire_tests-coverage.json) |
<!-- module-table:end -->

</details>
