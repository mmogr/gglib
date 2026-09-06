# remote

<!-- module-docs:start -->

Handlers for the remote tunnel ([ADR 0012](../../../../../docs/adr/0012-the-remote-tunnel.md)):
enable, disable, status. Thin: each maps one `RemoteOps` call onto the wire.

Two decisions live here rather than in `RemoteOps`:

- **Enable is not idempotent.** A second `enable` while the tunnel is up is a
  `409`. Answering it would mean minting a second pairing code for a live
  session or re-reading the first, and the response is the only place the
  ticket and the code are ever shown — a `GET` that could return them would
  make them retrievable by anything that can call `GET`.
- **Disable is.** A tunnel that is already down is the outcome asked for, so
  the handler answers with the status rather than a conflict.

`wire` holds the shapes. The status carries the ticket's fingerprint and never
the ticket; the peers by fingerprint; and the counters the tunnel's owner
keeps. `ts-rs` exports them for the Remote panel.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`wire.rs`](wire.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-remote-wire-coverage.json) |
<!-- module-table:end -->

</details>
