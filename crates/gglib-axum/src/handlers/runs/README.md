# runs

<!-- module-docs:start -->

`/api/runs/*`: this machine's door to the daemon's runs, replies it owns
until they end. Every call is made in the `Local` scope, so this door lists
and cancels every run but cannot read a paired device's reply.

# Module Layout

```text
runs/
  mod.rs   — start, list, read and cancel a run
  sse.rs   — a run's events as server-sent events: `id: <seq>` and
             `data: <frame>`, then one `event: run` with the run's final
             state, ending early when the daemon stops
```

<!-- module-docs:end -->
