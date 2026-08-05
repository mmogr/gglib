# Up

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-complexity.json)

<!-- module-docs:start -->

`gglib up` — the first-run path, from a clean machine to a working
OpenAI-compatible endpoint in one command.

This module **composes**; it implements nothing that already exists elsewhere.
Every step delegates, which is what keeps `up` in step with the commands it
stands in for.

| Step | Delegates to |
|------|--------------|
| 1. Hardware | [`gglib_app_services::SetupOps::get_status`] — the same status the GUI wizard renders |
| 2. llama.cpp | [`gglib_runtime::llama::ensure_llama_initialized`], as `gglib serve` uses |
| 3. Model | [`gglib_core::domain::recommend`], then `DownloadManagerPort::queue_smart` + the shared interactive monitor |
| 4. Proxy | [`gglib_runtime::proxy::start_proxy_standalone`], unpinned — identical to `gglib proxy` |
| 5. Warm-up | one HTTP request to the endpoint it just started |

## Why the warm-up is an HTTP request

An unpinned proxy loads nothing until traffic arrives, so without a first
request the launch narration never prints and `up` ends on a bound socket
rather than evidence. Going over HTTP — rather than reaching into the runtime —
exercises the router, the contention gate, model resolution and the forward
pipeline, so the endpoint is demonstrated to work rather than assumed to.

It runs as a spawned task because `start_proxy_standalone` blocks until
shutdown, and it never fails the command: a slow first load leaves a perfectly
usable proxy, and tearing that down would be worse than the cold start it was
avoiding.

## Not responsible for

Configuring anything. `up` binds loopback with gglib's defaults and takes three
flags (`--yes`, `--model`, `--port`). Host, upstream port, sampling and cache
behaviour belong to `gglib proxy`.

## Modules

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`choose.rs`](choose.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-choose-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-choose-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-choose-coverage.json) |
| [`probe.rs`](probe.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-probe-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-probe-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-probe-coverage.json) |
| [`warm.rs`](warm.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-warm-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-warm-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-up-warm-coverage.json) |
<!-- module-table:end -->

<!-- module-docs:end -->
