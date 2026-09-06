# remote

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-complexity.json)

<!-- module-docs:start -->

The remote tunnel — the serve side of [ADR 0012](../../../../docs/adr/0012-the-remote-tunnel.md).

`RemoteOps` puts a `modelpipe` listener in front of the running proxy so a
paired machine reaches `http://127.0.0.1:<port>/v1` here from anywhere, over
an end-to-end encrypted p2p connection. It lives in this crate for the reason
`ProxyOps` does: every `*Ops` type is here, the CLI and the GUI are thin
clients over the daemon API, and nothing above this crate ever sees an iroh
type.

# Module Layout

```text
remote/
  mod.rs      — RemoteOps: enable / disable / status, and the rotation poll
  gateway.rs  — RemoteGateway: the port the proxy asks (redeem a code,
                is /mcp open, a tunnelled request arrived)
  pairing.rs  — the one-time code: begin, redeem once, burn on the third miss
  key.rs      — which key the tunnel enforces, or that one must be minted
```

# One key, two doors

The listener enforces the **same** bearer token the proxy enforces
(`TokenPolicy::Supplied`). A wrong token is refused at the tunnel edge before
a byte reaches the daemon, and again by the proxy's own guard if it got there.

`key.rs` decides which token that is, in order: what the running proxy
actually demands (a `--api-key` flag is pinned and never appears in settings,
so the stored value would be wrong); the stored `proxy_api_key`; or a fresh
key, persisted. The last case is the loopback default — nothing minted a key
because nothing was reachable — and `enable` waits one settings-cache window
after writing it so the local door is locked before a ticket exists. That
wait is also the one behaviour change a local client will notice: the loopback
proxy now demands the key too, and disabling the tunnel does not take that
away.

Rotation has no event to hook. The CLI writes the same SQLite file from
another process, so `RemoteOps` polls `proxy_api_key` on the settings cache's
cadence and calls `ServeHandle::set_token` when it changes; a pinned key is
never watched, because nothing in settings may override it.

# Pairing

`enable` returns the ticket and a six-digit code exactly once. The code is
granted at the tunnel edge (`grant_once`) so one request bearing it gets
through without the token; the proxy's pairing route asks `RemoteGateway`
whether it is the code this session minted, and takes the key it stands for.
Two minutes, one redemption, three wrong attempts. Every refusal is the same
refusal.

# What the gateway is for

`RemoteGateway` is always installed, tunnel up or not, because `ProxyOps`
attaches it to every proxy it starts. With nothing armed it rejects every
code. It also holds the `/mcp` grant for tunnelled requests — off unless
`enable` was asked for it — and counts tunnelled requests for the status
surface. Its `Debug` reports state and never a code or a key.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`gateway.rs`](gateway.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway-coverage.json) |
| [`gateway_tests.rs`](gateway_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway_tests-coverage.json) |
| [`key.rs`](key.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-key-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-key-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-key-coverage.json) |
| [`pairing.rs`](pairing.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing-coverage.json) |
| [`pairing_tests.rs`](pairing_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_tests-coverage.json) |
| [`rotation.rs`](rotation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-rotation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-rotation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-rotation-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-types-coverage.json) |
<!-- module-table:end -->

</details>
