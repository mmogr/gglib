# runs

<!-- module-docs:start -->

The daemon's runs, replies it owns until they end, as HTTP serves them.

# Module Layout

```text
runs/
  mod.rs   — the module
  sse.rs   — a run's events as server-sent events: `id: <seq>` and
             `data: <frame>`, then one `event: run` with the run's final
             state, ending early when the server stops; shared with the
             daemon's `/api/runs`
```

<!-- module-docs:end -->
