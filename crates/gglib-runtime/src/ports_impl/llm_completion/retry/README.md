# Retry

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-retry-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llm_completion-retry-complexity.json)

<!-- module-docs:start -->

Client-side retry for transient upstream failures on the completions request.

The proxy queues a request whose model is not loaded, and answers `503` with a
`Retry-After` header only once that wait outlasts its deadline — see
[admission](crate::process::admission). This module is the caller that honours
it. It covers every in-process consumer at once — CLI agent chat and the GUI's
server-side agent loop both reach the upstream through this one adapter.

Backoff shape is not decided here: it comes from
[`gglib_core::retry`](gglib_core::retry), which the proxy's own `Retry-After`
hint is derived from, so the two ends agree by construction.

# The idempotency window

Retrying is confined to the phase before any response body is read. `classify`
hands a successful response back unread and the caller only then builds the
stream decoder, so a retry can never replay tokens the user has already seen or
re-trigger a tool call. The window closes the instant a 2xx is returned.

Transport failures — refused connection, send timeout — stay terminal, as they
were before this module existed.

# Classification

Three structured signals, with no inspection of human-readable message text at
any point:

1. **The error body's `type`.** The proxy sends `ErrorResponse`, whose `type`
   discriminant is resolved through
   [`is_retryable_error_type`](gglib_core::ports::model_runtime::is_retryable_error_type)
   — the same predicate the IPC surface uses, so HTTP and IPC cannot disagree
   about what is worth retrying, and a new retryable variant needs no change
   here.
2. **The error body's `code`.** A remote turn is fronted by modelpipe, which
   writes the same envelope with no `type` key at all and distinguishes its
   refusals by `code` alone. Only `tunnel_unavailable` is retryable: it means
   the connect side is between tunnels — the laptop moved networks and
   `keep_connected` is dialling — and it heals in seconds. The other two
   gateway refusals report the far machine's model server and are terminal.
   All three are `502`, so the status below cannot separate them.
3. **The HTTP status.** When the adapter targets a llama-server directly the
   body is not ours to interpret, so `503` and `429` alone drive the decision.

Only the third is ranked. The first two are read as a disjunction: either one
naming a retryable condition is enough, so a body carrying a terminal `type`
alongside a retryable `code` would be retried on the `code`. No body written
today carries both, but nothing in the code enforces that. The status decides
alone whenever the body offers neither field — a `500` blob and a bare
`{"error":{"message":…}}` get the same answer, which is the answer the status
gives.

# What a terminal failure says

Classification renders one string for logs, and until now that string was also
the whole of what the caller was told. That is enough for a local upstream and
not enough for a remote one: a laptop whose stored key has gone stale — the
desktop rotated its `proxy_api_key` — got `401 Unauthorized invalid_api_key:
invalid or missing bearer token`, which names no machine and offers no remedy.

So the classified failure carries the upstream's own `code` out beside the
rendered reason, and the far machine — passed in by whichever surface chose
the remote upstream, and the only thing here that knows one was involved — is
offered the chance to say something better before the reason is used. Today
that is only `invalid_api_key`, which becomes a sentence naming the machine by
its ticket fingerprint and pointing at `gglib remote enable` on it.

Both halves of that condition are load-bearing. The code alone is not enough:
gglib's own proxy answers a bad key with the same `invalid_api_key`, and
telling someone to re-pair a machine that is not in the picture is worse than
saying nothing. Being remote alone is not enough either: a far machine whose
model server is down fails terminally too, and its key is fine. The rendered
sentence carries no status and none of the upstream's own wording, so the
rendered reason is written to the log at that point rather than left to the
caller.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`bearer_tests.rs`](bearer_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-bearer_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-bearer_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-bearer_tests-coverage.json) |
| [`classify.rs`](classify.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify-coverage.json) |
| [`classify_tests.rs`](classify_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify_tests-coverage.json) |
| [`execute.rs`](execute.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute-coverage.json) |
| [`execute_tests.rs`](execute_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute_tests-coverage.json) |
| [`headers.rs`](headers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers-coverage.json) |
| [`headers_tests.rs`](headers_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers_tests-coverage.json) |
| [`refusal_tests.rs`](refusal_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-refusal_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-refusal_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-refusal_tests-coverage.json) |
| [`test_server.rs`](test_server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-test_server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-test_server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-test_server-coverage.json) |
<!-- module-table:end -->

</details>
