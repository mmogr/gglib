# runs

<!-- module-docs:start -->

`/v1/runs/*`: a paired device's door to the daemon's runs, replies the
daemon owns until they end. The same five calls, bodies and event framing as
the daemon's `/api/runs/*`, served through the `RunsPort` the proxy was
started with, and answered `503 runs_unavailable` without one.

A request the tunnel edge marked is served in the scope of the device it
named, and sees only that device's runs; any other request is this
machine's. The edge reaches the proxy as any client does, so a client
that reaches the proxy directly can forge the markers and be taken for a
device:
this machine not reading a device's reply is a courtesy, not a boundary.

# Module Layout

```text
runs/
  mod.rs       — the module
  handlers.rs  — start, list, read, follow and cancel a run, with errors in
                 the proxy's shape and the registry's codes; nothing a client
                 sent is echoed
  scope.rs     — who is asking, from the `Tunnelled` marker
  sse.rs       — a run's events as server-sent events: `id: <seq>` and
                 `data: <frame>`, then one `event: run` with the run's final
                 state, ending early when the server stops; shared with the
                 daemon's `/api/runs`
```

<!-- module-docs:end -->
