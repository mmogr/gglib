# Daemon Client

<!-- module-docs:start -->

The CLI's side of the daemon contract: probe, auto-launch, and typed calls
against the management API on `127.0.0.1:{DAEMON_PORT}`.

[`ensure_daemon`] is the entry point every runtime-owning command goes
through: probe `/health` for the `gglib-daemon` identity marker; if nothing
answers, spawn `gglib daemon run` detached (own process group, output to
`<data_root>/logs/daemon.log`) and poll until it is up. A port held by
something that is *not* a gglib daemon is a hard error, never fought over.

This module is responsible for finding or starting the daemon and for the
thin request wrappers commands share. It is **not** responsible for
rendering — handlers own their output — and it never falls back to
instantiating a local runtime: single process ownership is the point.

<!-- module-docs:end -->

<!-- module-table:start -->
| Module | Tests | Coverage | LOC | Complexity |
|--------|-------|----------|-----|------------|
<!-- module-table:end -->
