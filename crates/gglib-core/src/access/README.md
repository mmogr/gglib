# Access

<!-- module-docs:start -->

Who may reach the proxy, and how they prove it.

Three gates, carried together in [`ProxyAccessConfig`] because one router
applies all three:

| Gate | Default | Answers |
|---|---|---|
| Bearer token | off | *is this client authorised?* |
| Host allowlist | always on | *did this client know where the proxy lives?* |
| Origin check | always on | *is a page on another site asking for a change?* |

Everything here is pure — predicates and data. The middleware that applies it
lives in `gglib-proxy`, which is the crate allowed to depend on axum.

# Why a Host allowlist, when CORS already exists

`CorsConfig::LocalOnly` inspects `Origin`, and DNS rebinding does not change
`Origin` — it changes which IP a hostname resolves to. An attacker's page stays
`https://evil.com` throughout, so the CORS predicate does reject it.

What CORS does not do is stop the request from being **sent**. It governs
whether the response may be **read**. For a preflighted request (anything
sending `Content-Type: application/json`) the browser asks permission first and
never sends the real request, so those are genuinely blocked. A simple request,
however, is sent, runs to completion, and only its response is withheld — any
side effect has already happened.

The `Host` header is the part rebinding cannot forge: the browser sends the
name the page asked for, which is the attacker's. Checking it closes the
simple-request gap for a rebound page, on every route, preflighted or not, and
removes that defence's dependence on CORS being configured correctly. A page
that posts to loopback directly sends a `Host` the allowlist admits; that is
the Origin check's business (below). The Host check is enforced
unconditionally, including when no token is set, because it costs a string
comparison and defends the case where the operator configured nothing.

# Why an Origin check, when the Host allowlist exists

A page does not need to rebind anything to reach loopback: it can post to
`http://127.0.0.1:8080` directly, and the browser sends `Host: 127.0.0.1:8080`,
which the allowlist admits. The page picks its own content type, so a form
post or a `no-cors` fetch with a `text/plain` body is not preflighted, and a
route that reads raw bytes, or reads no body at all, runs it.

The browser does say which page is asking, in `Origin`. [`may_change`] reads
it for every method but `GET`, `HEAD` and `OPTIONS`, and admits an origin when
the router's [`CorsConfig`] would let that page read the answer, so a page
that names any origin but the endpoint's own may change something exactly when
the CORS layer lets it read the answer; `null`, which a page sends to hide its
origin, may change nothing, even under a config that lets every page read and
so answers it. An origin naming the very host the request was sent to is the
endpoint's own page and passes too, which is sound only because the Host
allowlist runs first. Programs send no `Origin` and pass, unless fetch metadata
says the request is cross-site.

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
