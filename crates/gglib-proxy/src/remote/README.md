# Remote

<!-- module-docs:start -->

The proxy's side of the remote tunnel ([ADR 0012](../../../../docs/adr/0012-the-remote-tunnel.md)):
one middleware that reads the tunnel's markers, one gate on `/mcp`, and one
route that redeems a pairing code for the key.

The proxy never sees the tunnel. What it has is a
[`RemoteGatewayPort`](gglib_core::ports::RemoteGatewayPort) on its
`AppState`, attached by whoever started it, and three questions it may ask:
is this code the one this session minted, may a tunnelled request reach
`/mcp`, and here is a request that came through the tunnel.

# The markers

The serve side sets `Via: 1.1 modelpipe` and `X-Modelpipe-Peer: <fingerprint>`
on every request it forwards, after removing any copy the client sent.
[`remote_marker`] reads them into a [`Tunnelled`] extension and tells the
owner a request arrived.

**They are restrictive only.** A local client can write these headers too,
and what it gains is a refusal on `/mcp` and a tick on a counter. Nothing is
granted on the marker's say-so and nothing ever should be. The direction
that matters holds: a tunnelled peer cannot make its request look local,
because the edge overwrites rather than inherits.

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
so a guesser learns nothing about which guess was close; the owner's
three-attempt burn is the defence.

It sits outside the bearer group because it cannot demand the credential it
hands out, and inside the Host guard like everything else. Reaching it at all
requires the ticket and the tunnel edge's own one-time grant.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`marker.rs`](marker.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker-coverage.json) |
| [`marker_tests.rs`](marker_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-marker_tests-coverage.json) |
| [`mcp_guard.rs`](mcp_guard.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-mcp_guard-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-mcp_guard-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-mcp_guard-coverage.json) |
| [`pair.rs`](pair.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-pair-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-pair-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-remote-pair-coverage.json) |
<!-- module-table:end -->

</details>
