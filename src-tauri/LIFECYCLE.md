# Application Lifecycle & Shutdown

> This document described the pre-daemon architecture until the tray was moved
> across the same seam: an embedded Axum server on an ephemeral port, a
> `BackgroundTasks` handle to abort, an in-app watchdog thread, and cleanup
> that stopped llama-servers and cancelled downloads from the desktop process.
> None of that exists. The teardown it described now lives in the daemon
> (`crates/gglib-axum/src/daemon/`), which is the only process that owns
> llama-server.

## Startup

`main.rs`'s setup hook, in order:

1. **Connect to the daemon** — `daemon::Daemon::connect_or_launch` probes
   `127.0.0.1:{DAEMON_PORT}` for `{"service": "gglib-daemon"}`. If nothing
   answers it launches `gglib daemon run` detached, preferring a binary
   beside the app bundle and falling back to `$PATH`; with no binary at all it
   hosts the daemon composition in-process, behind the daemon's own file lock.
   This blocks for up to 15 seconds on a cold start.

   A failure here is **not fatal**. The app comes up with a disconnected
   `Daemon`, the tray reads "gglib — not running", and Start gglib Service is
   the way back. It used to `expect`, which killed the process before there was
   a tray or a window to explain it with.

2. **Build the tray** — before the application menu, because on Linux and
   Windows it is the only persistent UI once the window is hidden. A failed
   build is survivable and gates `autostart::should_start_hidden`.

3. **Apply initial visibility** — `autostart::apply_initial_visibility` hides
   the main window for a login launch with `close_to_tray` set. It hides a
   window declared visible rather than showing one declared hidden; see the
   Wayland/KWin note in `autostart.rs`.

4. **Start the daemon watcher** — `daemon::watch` polls `/api/proxy/status`
   and `/api/servers` every 2 seconds into a `DaemonSnapshot`. Its first poll
   is the initial paint for the tray and the macOS menu.

5. **Register the login item** — `autostart::apply`, matching `start_at_login`.
   Proxy autostart is deliberately absent: the daemon honours
   `proxy_autostart` itself, whoever started it.

## Shutdown

Three paths, one sequence. `lifecycle::request_shutdown` is the single entry
point, guarded by a `swap` on `SHUTTING_DOWN` so overlapping requests collapse
into one:

| Trigger | Path |
|---|---|
| Tray → Quit gglib | confirm, then `request_shutdown` |
| Cmd+Q (`RunEvent::ExitRequested`) | `should_prevent_exit`, then `request_shutdown` |
| Window close, `close_to_tray` off | `request_shutdown` |
| Window close, `close_to_tray` on | **hide only** — nothing is torn down |

`ExitRequested` is prevented exactly once. `AppHandle::exit` re-enters it, and
preventing that second one is what used to strand the process alive with a dead
backend.

### What quitting takes with it

`perform_shutdown` asks `daemon::Ownership`:

| Ownership | On quit |
|---|---|
| `Launched` — this app spawned `gglib daemon run` | Stopped |
| `Hosted` — running inside this process | Stopped |
| `Adopted` — already answering when we connected | **Left running** |
| `Unresolved` — nothing ever answered | Nothing to stop |

Stopping means `POST /api/daemon/shutdown` and a bounded wait. The ordered
teardown — proxy drained, every llama-server stopped with SIGTERM then SIGKILL,
downloads cancelled, a final pidfile audit — runs *in the daemon*, under its own
10-second force-exit watchdog. The app waits 12 seconds, deliberately longer, so
it outlasts the deadline the daemon holds itself to rather than racing it.

Quit means quit. `close_to_tray` is the verb for keeping the endpoint up without
a window, so an exit that quietly did the same thing was one button doing
another's job — and it left VRAM spoken for with the tray icon gone and nothing
on screen to say so.

The exception exists because a daemon may not be this app's to end: a
`gglib proxy` running in a terminal that the GUI merely connected to keeps
serving. **Stop gglib Service** on the tray is the explicit way to end one
without quitting.

> **Known staleness:** ownership is decided once, at connect. An app that
> launched a daemon a CLI session later started using still reads `Launched`,
> so quitting stops it. Knowing better needs the daemon to report who is
> attached — `SseBroadcaster::subscriber_count` exists but is not exposed.

## Testing

```bash
# Iterate: open, close, and confirm nothing is left behind.
gglib gui
# ...quit from the tray...
gglib daemon status          # not running, if the app launched it
pgrep -fl llama-server       # empty

# The adopted case: this daemon must survive the GUI.
gglib proxy &
gglib gui
# ...quit...
gglib daemon status          # still running
```

## Code References

- `src-tauri/src/lifecycle.rs` — the shutdown sequence and the ownership rule
- `src-tauri/src/daemon/mod.rs` — connect, launch, `Ownership`, restart
- `src-tauri/src/daemon/watch.rs` — the poller that keeps every surface true
- `src-tauri/src/main.rs` — setup hook, window events, `RunEvent` handling
- `src-tauri/src/autostart.rs` — login item and initial visibility
- `crates/gglib-axum/src/daemon/` — the teardown itself, and its watchdog
