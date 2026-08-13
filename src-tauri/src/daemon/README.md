# daemon

<!-- module-docs:start -->

The desktop app's connection to the gglib daemon, and the one task that keeps
its picture of it true.

The daemon — not this app — owns llama-server. On startup `connect_or_launch`
probes the fixed daemon port; if nothing answers it launches `gglib daemon run`
detached when a CLI binary can be found, and otherwise hosts the same daemon
composition in-process as a fallback for bundle-only installs, still behind the
daemon's own file lock. A daemon that will not start is not fatal:
`Daemon::disconnected` lets the app come up anyway, so the tray exists to say
so and to offer a way back.

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

It is decided once, at connect, and can go stale: a daemon this app started
that a CLI session later began using still reads `Launched`. Knowing better
needs the daemon to report who is attached.

## Snapshot and watch

`snapshot` is what the daemon reports; `watch` is the task that polls it every
couple of seconds and repaints every surface when — and only when — it changed.

Nothing in this process used to ask the daemon anything, so the tray showed
whatever it had last done itself and a proxy started by `proxy_autostart`, the
CLI, or the window left it wrong for the whole session.

Polling rather than subscribing to `/api/events` is deliberate. The server
lifecycle events are deltas; `gglib-sse`'s broadcaster drops them silently for
a lagging subscriber, and its one `ServerSnapshot` goes out at daemon startup
rather than per subscriber — so a single missed event would leave the resident
count wrong until the app restarted. Every poll is absolute, so drift cannot
accumulate; a failed request is also the daemon-down signal, and it catches a
wedged daemon that a 30-second SSE keep-alive would still call healthy.

The watcher is the **only** writer. Callers that change something ask it for an
immediate poll rather than publishing what they expect to be true, because an
optimistic write next to a poll is a lost update: a request that read before
the action can land after it.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
