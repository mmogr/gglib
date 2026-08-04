# web

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-web-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-web-complexity.json)

<!-- module-docs:start -->

Web server command handler.

Handles starting the Axum HTTP server with optional static file serving.
Discovers frontend build artifacts automatically from well-known paths,
or falls back to API-only mode when no frontend is present.

Bind-address and CORS resolution lives in `bind`; this module orchestrates
it, prints the startup banner, and blocks on the server. When LAN sharing is
active it also advertises the server over mDNS (`mdns`) and races the server
against a shutdown signal (`shutdown`) so the record is withdrawn on the way
out.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`bind.rs`](bind.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-bind-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-bind-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-bind-coverage.json) |
| [`mdns.rs`](mdns.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-mdns-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-mdns-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-mdns-coverage.json) |
| [`shutdown.rs`](shutdown.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-shutdown-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-shutdown-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-web-shutdown-coverage.json) |
<!-- module-table:end -->

</details>
