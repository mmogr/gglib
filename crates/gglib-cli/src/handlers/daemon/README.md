# Daemon

<!-- module-docs:start -->

Handlers for the `gglib daemon` command group.

`run` hosts [`gglib_axum::daemon::run_daemon`] in the foreground — the same
composition every auto-launched daemon runs, plus the LAN mode
(`--share-lan`: wildcard bind, relaxed CORS, mDNS advertising, and a loud
warning). `status` combines the health probe, the lock file's recorded
holder, and the proxy status into one report. `stop` asks the daemon to shut
down over the API and waits for it to actually go away.

This module owns presentation only; the daemon's behaviour — lock, sweep,
signals, teardown — lives in `gglib-axum`'s `daemon` module.

<!-- module-docs:end -->

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`mdns.rs`](mdns.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-daemon-mdns-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-daemon-mdns-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-daemon-mdns-coverage.json) |
<!-- module-table:end -->
