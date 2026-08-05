# Daemon

<!-- module-docs:start -->

The gglib daemon: the one process per machine that owns llama-server.

[`run_daemon`] composes the pieces the rest of this crate already provides —
[`bootstrap`](crate::bootstrap::bootstrap) (which builds the single
`ProcessManager`), the management router, and the proxy supervisor reachable
through `ProxyOps` — into a long-lived background process bound to
`127.0.0.1:{DAEMON_PORT}`. Every other surface is a client: the CLI talks to
the daemon's HTTP API, the desktop app is a dashboard over the same API, and
OpenAI-compatible clients hit the proxy the daemon supervises.

Single ownership is enforced by [`DaemonLock`]: an exclusive OS file lock on
`<data_root>/daemon.lock`, taken before anything else runs and released by
the OS on any process death, so no stale-lock recovery is needed. The fixed
port is the second line of defence — a foreign process on the port is
reported, not fought.

This module is responsible for:

- the singleton lock and the "already running (pid N)" refusal,
- sweeping orphaned llama-server pidfiles at startup (moved here from the
  desktop app, which used to kill a concurrent CLI's servers),
- SIGINT/SIGTERM handling and the `POST /api/daemon/shutdown` route's
  cancellation token,
- ordered teardown: proxy drained, every llama-server child stopped
  gracefully (SIGTERM → grace → SIGKILL), downloads cancelled, a final
  pidfile audit — all under a force-exit watchdog.

It is **not** responsible for deciding *when* to run: clients auto-launch
`gglib daemon run` on demand (see `gglib-cli`'s `daemon_client`), and the
desktop app falls back to hosting [`run_daemon`] in-process when no CLI
binary can be found — still behind the same lock.

<!-- module-docs:end -->

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`lock.rs`](lock.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-lock-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-lock-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-lock-coverage.json) |
| [`shutdown.rs`](shutdown.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-shutdown-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-shutdown-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-daemon-shutdown-coverage.json) |
<!-- module-table:end -->
