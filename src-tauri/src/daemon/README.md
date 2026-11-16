# daemon

<!-- module-docs:start -->

The desktop app's connection to the gglib daemon, and the one task that keeps
its picture of it true.

This module is the one place in this crate that knows the daemon's base URL:
every request the app's Rust code sends the daemon is built on `base_url`, and
carries the daemon's token, read from its file by `token.rs`.

The daemon — not this app — owns llama-server. On startup `connect_or_launch`
probes the fixed daemon port; if nothing answers it launches `gglib daemon run`
detached when a CLI binary can be found, and otherwise hosts the same daemon
composition in-process as a fallback for bundle-only installs, still behind the
daemon's own file lock. A daemon that will not start is not fatal: the app is
already up, the tray says so and offers a way back.

## Service: known once the launch has an answer

The app comes up before its daemon does. `AppState` is managed on the builder,
before `build` creates the windows, and `setup` starts the launch
(`daemon::start`) beside the window instead of waiting for it. `service.rs`
owns the one question that leaves open, where the daemon is and whose it is:
while a launch (or a Start gglib) is in flight, `Service::settled` waits for
its answer. The page's `get_embedded_api_info` asks it, so the page's first
requests go to a daemon that listens, with the token it minted; quitting asks
it, so a daemon this app has just spawned is not left behind as unclaimed.

Every launch settles: its `Launch` hands over the outcome, and one dropped
unsettled (a panicked task) settles as a failure. The launch's own budget
(`LAUNCH_WAIT`, plus its first probe and its last poll: a little over 15 s)
is therefore the bound on every wait, with no second clock. A second start
while one is in flight joins it rather than spawning another, and leaves
reporting a failure to the launch's owner; nothing is reported once the app
is shutting down. A failed launch is not final: a daemon the watcher hears
answering later is found. A failure keeps the ownership the launch had, so a
failed Start gglib after this app's own daemon stopped still owns what it
spawned, and quitting stops it; a first launch that failed finds a late
daemon (or one started from a terminal) as not this app's to stop.

## The service's process id

`holder.rs` reads it from the record the service's lock writes beside it
(`daemon.json`), only for the port this app talks to; `/health` does not
say it. The lock is not touched (checking it would take it for an
instant): `allow_service_foreground` reads the record only when the
watcher's last poll reached the service, and waits for no launch. It lends
that process the right to come to the front as a repair is posted, so
Windows' permission prompt for the installer it runs is not left on the
taskbar. A service killed since the last poll leaves its record, and its
id is read until the poll notices.

## Ownership

Which of those three happened is kept, because it decides what quitting is
allowed to take with it:

| | |
|---|---|
| `Adopted` | Already answering when we probed — someone else's. |
| `Launched` | We spawned `gglib daemon run`. |
| `Hosted` | Running inside this process. |
| `Unresolved` | Nothing ever answered. |

`ends_with_the_app` is the rule: the middle two are ours to stop, the others
are not. This was a single `hosted_in_process` bool, which could only ask "is
it in this process" — so quitting left the ordinary case, an external daemon
this app had launched itself, running and serving after a dialog had said it
would stop.

It is decided once, when the launch settles (unknown, and waited for, while it
is in flight), and can go stale: a daemon this app started
that a CLI session later began using still reads `Launched`. Knowing better
needs the daemon to report who is attached.

## Snapshot and watch

`snapshot` is what the daemon reports; `watch` is the task that polls it every
couple of seconds and repaints every surface when — and only when — it changed.
While a launch is in flight it paints "starting" and waits for the answer, so
no surface calls a daemon on its way "not running".

Nothing in this process used to ask the daemon anything, so the tray showed
whatever it had last done itself and a proxy started by `proxy_autostart`, the
CLI, or the window left it wrong for the whole session.

Polling rather than subscribing to `/api/events` is deliberate. The server
lifecycle events are deltas; `gglib-broadcast` drops them silently for a
lagging subscriber, and no event carries the whole picture — so a single
missed event would leave the resident count wrong until the app restarted.
Every poll is absolute, so drift cannot accumulate; a failed request is also
the daemon-down signal, and it catches a wedged daemon that a 30-second SSE
keep-alive would still call healthy.

The watcher is the **only** writer. Callers that change something ask it for an
immediate poll rather than publishing what they expect to be true, because an
optimistic write next to a poll is a lost update: a request that read before
the action can land after it.

<!-- module-docs:end -->
