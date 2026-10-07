# Daemon Client

<!-- module-docs:start -->

The CLI's side of the daemon contract: probe, auto-launch, and typed calls
against the management API on `127.0.0.1:{DAEMON_PORT}`.

[`ensure_daemon`] is the entry point every runtime-owning command goes
through: probe `/health` for the `gglib-daemon` identity marker; if nothing
answers, spawn `gglib daemon run` detached (own process group, output to
`<data_root>/logs/daemon.log`) and poll until it is up. A port held by
something that is *not* a gglib daemon is a hard error, never fought over.

[`running`] is the same probe for a command that only reports on the daemon,
or stops something on it: it hands back the daemon when one is up, says what
is there instead when none is, and never launches one. Either way the
[`DaemonHandle`] carries the credential [`auth::daemon_api_key`] resolves, and
every call made through it sends that credential, so no command resolves or
attaches one itself.

This module is responsible for finding or starting the daemon and for the
thin request wrappers commands share. It is **not** responsible for
rendering — handlers own their output — and it never falls back to
instantiating a local runtime: single process ownership is the point.

`repair.rs` holds the call that has the daemon repair a model.
`runs.rs` holds the run calls and reads a run's event stream, whose events
`drain_items` turns into numbered frames and the run's final state. `sse.rs`
reads a stream of JSON events, such as a benchmark's. Neither cuts its stream
into events itself: `gglib_core::sse::DataFrames` does, for these two and for
`gglib proxy dashboard`.

<!-- module-docs:end -->
