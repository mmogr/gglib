# runs

<!-- module-docs:start -->

`/api/runs/*`: this machine's door to the daemon's runs, replies it owns
until they end. Every call is made in the `Local` scope, so this door lists
and cancels every run but cannot read a paired device's reply.

# Module Layout

```text
runs/
  mod.rs   — start, list, read and cancel a run; a run's events are framed
             by `gglib_proxy::runs::sse`, which the proxy's door shares;
             `?kind=agent` starts an agent run (`handlers/agent/run.rs`),
             which only this door can
```

<!-- module-docs:end -->
