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

Two structured signals, in order of authority, with no inspection of
human-readable message text at any point:

1. **The error body.** The proxy sends `ErrorResponse`, whose `type`
   discriminant is resolved through
   [`is_retryable_error_type`](gglib_core::ports::model_runtime::is_retryable_error_type)
   — the same predicate the IPC surface uses, so HTTP and IPC cannot disagree
   about what is worth retrying, and a new retryable variant needs no change
   here.
2. **The HTTP status.** When the adapter targets a llama-server directly the
   body is not ours to interpret, so `503` and `429` alone drive the decision.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`classify.rs`](classify.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-classify-coverage.json) |
| [`execute_tests.rs`](execute_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute_tests-coverage.json) |
| [`execute.rs`](execute.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-execute-coverage.json) |
| [`headers_tests.rs`](headers_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers_tests-coverage.json) |
| [`headers.rs`](headers.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-headers-coverage.json) |
| [`test_server.rs`](test_server.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-test_server-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-test_server-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-retry-test_server-coverage.json) |
<!-- module-table:end -->

</details>
