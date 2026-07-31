# Contention

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-complexity.json)

<!-- module-docs:start -->

Bounded server-side wait for model-startup contention.

When a request arrives for one model while another is still starting, the
runtime can answer with `ContentionTimeout`. Failing fast on that turns a
recoverable resource collision into a `503`, and for an OpenAI-compatible
client a `503` is terminal — the GitHub Copilot LLM Gateway and VS Code both
abandon the request rather than backing off. A slow `200` is strictly better
for them than a fast `503`.

So the proxy absorbs contention for a bounded window first, and only surfaces
the `503` if the window elapses while still contended. Clients that *do* honour
`Retry-After` — this workspace's own completion adapter among them — still get
one, with an `x-gglib-retry-reason: contention` header distinguishing it from
ordinary model loading.

Polling backoff comes from [`gglib_core::retry`](gglib_core::retry), the same
policy the client-side adapter uses, so the two ends of the same failure agree
on shape rather than drifting apart.

# Scope

Only `ContentionTimeout` is waited on. `ModelLoading` keeps the caller's own
longer retry schedule and is returned untouched, so nothing outside the
contention path changes.

# Configuration

`GGLIB_CONTENTION_WAIT_SECS` overrides the 30-second default, resolved once
when the proxy's state is built. Zero restores fail-fast: the `503` goes
straight to the client.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`wait_tests.rs`](wait_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-wait_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-wait_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-wait_tests-coverage.json) |
| [`wait.rs`](wait.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-wait-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-wait-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-contention-wait-coverage.json) |
<!-- module-table:end -->

</details>
