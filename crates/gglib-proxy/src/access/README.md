# Access

<!-- module-docs:start -->

The three request guards, as axum middleware. The policy they enforce —
which hosts, which pages, which token — is [`gglib_core::access`], which is
pure; this module is only the part that needs to know what an HTTP request is.

# Layer order

The guards are installed in `router.rs`, and the order is load-bearing:

```text
CorsLayer              outermost — answers OPTIONS preflight and returns
  host_guard           every route, every path, always on
    origin_guard       every route; asks only of a change
      (route match)
        bearer_guard   matched routes only; asks a token only when one is set
          handler
```

`CorsLayer` has to stay outside every guard. It short-circuits a preflight `OPTIONS`
itself, and a preflight carries neither the eventual request's credentials nor
anything else worth checking — letting it reach `bearer_guard` would answer
every preflight with a `401` and break browser clients that are perfectly
entitled to connect.

`host_guard` sits outside the router rather than on the routes, so it also
covers `/health` and requests that match no route. It is a string comparison
against a short list; there is no reason to buy exemptions with it.

`origin_guard` sits inside `host_guard`. It trusts a page whose `Origin`
names the `Host` the request was sent to, which is safe only because the Host
guard refuses every name this proxy does not answer to. Any other page may
change something only if the CORS layer would let it read the answer: the
guard asks `CorsConfig::allows_origin`, and the CORS layer lets an origin read
exactly when it does.

`bearer_guard` is a `route_layer`, so `/health` — registered outside the
protected group — stays reachable without a token, and a 404 stays a 404
instead of becoming a 401 that tells an unauthenticated caller which paths
exist. It is installed whether or not a token is set, and decides on each
request which token, if any, it asks for; with none set it asks nothing.

# Failure responses

| Guard | Status | Code | Also |
|---|---|---|---|
| `host_guard` | 403 | `host_not_allowed` | names `--allowed-host` in the message |
| `origin_guard` | 403 | `origin_not_allowed` | `GET`, `HEAD` and `OPTIONS` are never refused |
| `bearer_guard` | 401 | `invalid_api_key` | `WWW-Authenticate: Bearer` |

All three use the `OpenAI` error envelope, which is what the rest of `/v1/*` already
speaks. `/mcp` is the exception on paper: it answers errors as JSON-RPC. A
middleware runs before the body is parsed, so it has no request `id` to echo
back and cannot construct a valid JSON-RPC error anyway — MCP clients key off
the status code, and 401/403 are unambiguous there.

# Comparing the token

`constant_time_eq` folds every byte rather than returning at the first
mismatch. A `==` leaks, through timing, how many leading bytes an attacker got
right, which over enough requests recovers the token a byte at a time. The
length is compared first and does leak, which is fine — the token's length is
not the secret.

<!-- module-docs:end -->
