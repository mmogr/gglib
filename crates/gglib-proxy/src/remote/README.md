# Remote

<!-- module-docs:start -->

The proxy's side of the remote tunnel ([ADR 0012](../../../../docs/adr/0012-the-remote-tunnel.md)):
one middleware that reads the tunnel's markers, and two gates that use what
it read. Pairing is not here: the tunnel edge answers a pairing request itself
and never forwards it.

The proxy never sees the tunnel. What it has is a
[`RemoteGatewayPort`](gglib_core::ports::RemoteGatewayPort) on its
`AppState`, attached by whoever started it, and two things it may say to it:
may a tunnelled request reach `/mcp`, and here is a request that came through
the tunnel.

# The markers

The serve side sets `Via: 1.1 modelpipe` and `X-Modelpipe-Peer: <fingerprint>`
on every request it forwards, after removing any copy the client sent, and
`X-Modelpipe-Device: <name>` naming the device token that admitted it. [`remote_marker`] reads all three into a [`Tunnelled`]
extension and tells the owner a request arrived.

The peer fingerprint comes from the connecting side's endpoint key, so what
it names is that side's choice: gglib's `join` keeps no key, so a laptop
arrives under a new one every time it connects, while ggchat keeps one per
machine it pairs with from 0.3.1. The device name is the durable identity,
because this side issued it.

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
tunnel. Under `TokenPolicy::Named` everything the edge admits names a device,
so today the gate refuses one thing: a request whose markers were forged to
look tunnelled by a client that reached the proxy directly. It stays as the only check left that could
refuse a credential a later modelpipe admits without naming a device, where
a mistake would reach `POST /v1/proxy/shutdown`.

# The `/mcp` gate

[`mcp_tunnel_guard`] is a `route_layer` on `/mcp` alone, inside the bearer
guard. A tunnelled request is refused with `403 mcp_not_allowed_over_tunnel`
unless the owner allows it — `gglib remote enable --allow-mcp`. With no owner
attached the answer is also no. `invoke_tool` starts the MCP servers
configured on this machine; a leaked token with a shell server configured is
remote code execution, which is not the same blast radius as free inference
and does not get the same default.

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
<!-- module-table:end -->

</details>
