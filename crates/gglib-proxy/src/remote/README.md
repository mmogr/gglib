# Remote

<!-- module-docs:start -->

The proxy's side of the remote tunnel ([ADR 0012](../../../../docs/adr/0012-the-remote-tunnel.md)):
one middleware that reads the tunnel's markers, two gates that use what it
read, and one route that redeems a pairing code for a device's key.

The proxy never sees the tunnel. What it has is a
[`RemoteGatewayPort`](gglib_core::ports::RemoteGatewayPort) on its
`AppState`, attached by whoever started it, and three questions it may ask:
is this code the one this session minted, may a tunnelled request reach
`/mcp`, and here is a request that came through the tunnel.

# The markers

The serve side sets `Via: 1.1 modelpipe` and `X-Modelpipe-Peer: <fingerprint>`
on every request it forwards, after removing any copy the client sent, and
`X-Modelpipe-Device: <name>` when a *named* token admitted it — never for a
one-time grant. [`remote_marker`] reads all three into a [`Tunnelled`]
extension and tells the owner a request arrived.

The peer fingerprint is minted per process on the connecting side, so it
names a run rather than a device: a laptop that restarts arrives under a new
one. The device name is the durable identity, because this side issued it.

**They are restrictive only.** A local client can write these headers too,
and what it gains is a refusal — on `/mcp`, or now from [`device_gate()`] — and
a tick on a counter. Nothing is granted on the marker's say-so and nothing
ever should be; a forged device name buys nothing, because the key it would
have to accompany is checked at the edge, not here. The direction that
matters holds: a tunnelled peer cannot make its request look local, because
the edge overwrites rather than inherits.

# The device gate

[`device_gate()`] is a `route_layer` on the protected group, inside the bearer
guard, refusing any tunnelled request the edge did not name a device for.

It exists because `ServeOptions::backend_auth` took the second door away.
The edge replaces the client's `Authorization` with the backend's own bearer
on every admitted request, so `bearer_guard` now validates a header modelpipe
wrote microseconds earlier and cannot refuse anything that crossed the
tunnel. That includes the request a **pairing grant** admits — and a grant is
one request at any path the holder likes, because the edge cannot scope it.
Without this gate a single guessed six-digit code would buy one fully
authenticated request to any protected route, `POST /v1/proxy/shutdown`
among them.

# The `/mcp` gate

[`mcp_tunnel_guard`] is a `route_layer` on `/mcp` alone, inside the bearer
guard. A tunnelled request is refused with `403 mcp_not_allowed_over_tunnel`
unless the owner allows it — `gglib remote enable --allow-mcp`. With no owner
attached the answer is also no. `invoke_tool` starts the MCP servers
configured on this machine; a leaked token with a shell server configured is
remote code execution, which is not the same blast radius as free inference
and does not get the same default.

# The pairing route

`POST /v1/remote/pair` with `{"code":"483920"}` answers `{"api_key":"…"}`
exactly once, and `401 invalid_pairing_code` for everything else — wrong,
expired, spent, burned, unparseable, or a proxy with no tunnel. One refusal,
so a guesser learns nothing about which guess was close. The owner's
three-attempt burn is the defence on the loopback path; over the tunnel a wrong
code is a wrong bearer, refused at modelpipe's edge before this route runs, so
the burn never fires there. ADR 0012 decision 3, amended 2026-09-07, records
what that leaves.

It sits outside the bearer group because it cannot demand the credential it
hands out, and inside the Host guard like everything else. From the far side
that means the ticket plus the edge's own one-time grant; from this machine's
loopback it means nothing at all, which is what the three-attempt burn is for.
A request the edge admitted on a *device* key is refused here before its code
is read: a machine that already holds one has no reason to pair, and leaving
it open would let a compromised device burn invites and guess at a second
identity.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`device_gate.rs`](device_gate.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-device_gate-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-device_gate-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-device_gate-coverage.json) |
| [`marker.rs`](marker.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker-coverage.json) |
| [`marker_tests.rs`](marker_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker_tests-coverage.json) |
| [`mcp_guard.rs`](mcp_guard.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-mcp_guard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-mcp_guard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-mcp_guard-coverage.json) |
| [`pair.rs`](pair.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-pair-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-pair-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-pair-coverage.json) |
<!-- module-table:end -->

</details>
