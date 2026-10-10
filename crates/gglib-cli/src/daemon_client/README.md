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
stops something on it, or asks it what only a running one knows (whether it
is replying to the chat `gglib chat --continue` names): it hands back the
daemon when one is up, says what is there instead when none is, and never
launches one. Either way the [`DaemonHandle`] carries the credential
[`auth::daemon_api_key`] resolves, and every call made through it sends that
credential, so no command resolves or attaches one itself.

This module is responsible for finding or starting the daemon and for the
thin request wrappers commands share. It is **not** responsible for
rendering — handlers own their output — and it never falls back to
instantiating a local runtime: single process ownership is the point.

`library_changes.rs` keeps the events a command's `ModelOps` emit for what
it changes in the library and, when the command is over, posts them to the
daemon that serves that library, so an app open on it shows the change. It
asks [`running`] for the daemon only when this data root holds a daemon's
token, which is then the credential presented: a daemon that serves this
library left it there, and a key says nothing of which library a daemon
serves. A daemon that is not there, or does not take the event, fails
nothing.

`generation.rs` is the daemon's generation gate as `gglib chat` sees it: a
turn is a connection to `GET /api/generation/turn`, held open until the turn
ends, so a reply sent to a llama-server port waits for an image render. The
gate's URL is fixed when it is made. No daemon, or one that does not answer,
gives a turn that holds nothing, said once; no render turn is granted here.

`repair.rs` holds the call that has the daemon repair a model.
`images.rs` is `DaemonImageGenerator`, core's image generation port over
the daemon's `POST /api/images/generations` with `stream: true`: progress
events become reports, completed events images, and an error event or a
refused request the daemon's code and words. `runs.rs` holds the run calls and reads a run's event stream, whose events
`drain_items` turns into numbered frames and the run's final state. `sse.rs`
reads a stream of JSON events, such as a benchmark's. Neither cuts its stream
into events itself: `gglib_core::sse::DataFrames` does, for these two and for
`gglib proxy dashboard`.

<!-- module-docs:end -->
