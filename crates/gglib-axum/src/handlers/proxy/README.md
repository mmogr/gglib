# proxy

<!-- module-docs:start -->

Handlers for the OpenAI-compatible proxy: status, start, start-pinned, stop.

`wire` holds the request and response shapes and the rules for resolving what a
caller left out — an omitted field means "use what is configured", not "use the
compile-time default", so a tray panel that sends no port must still land on
the same one as `gglib proxy`. Those rules are where the surprises live, and
they are testable without any routing around them.

What stays in `mod` is the routing and two idempotency decisions:

- **Start** treats an already-running proxy as success, because every caller
  wants an endpoint rather than exclusivity. But `ProxyOps::map_start_error`
  maps *both* `AlreadyRunning` and `BindFailed` to `Conflict`, and only the
  first leaves something listening — so the handler checks which happened
  instead of assuming. Reading a bind failure as success answered `200` with
  `running: false` and discarded the message naming the port that was taken.
- **Start-pinned** refuses when a proxy is already running under a different
  pin. Succeeding there would hand back an endpoint without the
  refuse-foreign-models guarantee the caller explicitly asked for.
- **Stop** treats an already-stopped proxy as success, symmetrically.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`wire.rs`](wire.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy-wire-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy-wire-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy-wire-coverage.json) |
| [`wire_tests.rs`](wire_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy-wire_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy-wire_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-proxy-wire_tests-coverage.json) |
<!-- module-table:end -->

</details>
