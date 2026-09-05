# Access

<!-- module-docs:start -->

Who may reach the proxy, and how they prove it.

Two gates, decided at bind time, carried together in [`ProxyAccessConfig`]
because the router applies both at the same layer:

| Gate | Default | Answers |
|---|---|---|
| Bearer token | off | *is this client authorised?* |
| Host allowlist | always on | *did this client know where the proxy lives?* |

Everything here is pure — predicates and data. The middleware that applies it
lives in `gglib-proxy`, which is the crate allowed to depend on axum.

# Why a Host allowlist, when CORS already exists

`CorsConfig::LocalOnly` inspects `Origin`, and DNS rebinding does not change
`Origin` — it changes which IP a hostname resolves to. An attacker's page stays
`https://evil.com` throughout, so the CORS predicate does reject it.

What CORS does not do is stop the request from being **sent**. It governs
whether the response may be **read**. For a preflighted request (anything
sending `Content-Type: application/json`, which is every `/v1/chat/completions`
and `/mcp` call) the browser asks permission first and never sends the real
request, so those are genuinely blocked. A simple `GET`, however, is sent, runs
to completion, and only its response is withheld — any side effect has already
happened.

The `Host` header is the part rebinding cannot forge: the browser sends the
name the page asked for, which is the attacker's. Checking it closes the
simple-request gap, covers any future route that is not preflighted, and
removes the endpoint's dependence on CORS being configured correctly. It is
enforced unconditionally, including when no token is set, because it costs a
string comparison and defends the case where the operator configured nothing.

# The allowlist

Loopback is a predicate, not a list — [`is_loopback_host`] accepts the literal
`localhost` and anything that parses as a loopback IP, so `127.0.0.2` and `::1`
work without anyone enumerating them.

Beyond loopback, [`ProxyAccessConfig::new`] admits exactly what the operator
named:

| Bind host | Also allowed |
|---|---|
| `127.0.0.1`, `localhost`, `::1` | nothing further — loopback covers it |
| `192.168.1.5` | `192.168.1.5` |
| `0.0.0.0`, `::` | nothing — a wildcard names no reachable address |

plus every `--allowed-host` value. The wildcard row is the one that breaks
existing setups, and it is deliberate: inferring the machine's interface
addresses would re-open the hole the check exists to close, so a wildcard bind
must name its hostname explicitly.

# The token

Optional. `None` leaves the endpoint behaving exactly as it did before
authentication existed, which is what keeps the upgrade silent for the loopback
default. [`ApiKeySource`] records where a set token came from so the startup
banner can explain the decision instead of merely stating it — and so a token
this process **generated** can be printed once, while one the operator already
holds is not echoed into terminal scrollback.

[`bearer_matches`] decides whether a request presents it. The auth scheme is
matched case-insensitively, because RFC 9110 says it is a token and tokens are
case-insensitive; only the credential goes to [`constant_time_eq`].

[`BearerPolicy`] decides *which* token is required, and it is a live question
rather than a bind-time one — a key rotated afterwards has to be honoured, and
a key set afterwards has to be enforced.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`bearer.rs`](bearer.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-bearer-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-bearer-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-bearer-coverage.json) |
| [`bearer_tests.rs`](bearer_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-bearer_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-bearer_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-bearer_tests-coverage.json) |
| [`host.rs`](host.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-host-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-host-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-host-coverage.json) |
| [`host_tests.rs`](host_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-host_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-host_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-access-host_tests-coverage.json) |
<!-- module-table:end -->

</details>
