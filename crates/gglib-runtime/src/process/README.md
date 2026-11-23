# process

<!-- module-docs:start -->

Process management infrastructure for every server gglib starts.

This module provides shared infrastructure for managing llama-server and
`sd-server` processes, with integrated log streaming and event broadcasting.

# Structure

- `GuiProcessCore` - Low-level process spawning with log streaming (u32 model IDs),
  for either runtime: a `SpawnConfig` names llama-server or `sd-server`, and
  `RuntimeBinaries` holds the binary each is started from
- `ProcessManager` - High-level orchestration; dispatch only
- `ResidentSet` (`residency/`) - The VRAM slots and the launch driver that fills them; tells a `ResidencyObserver` of every load and unload
- `AdmissionQueue` (`admission/`) - Decides who gets the GPU next, and for how long
- `ServerEvent` / `ServerEventBroadcaster` - Lifecycle event broadcasting
- `ServerLogManager` - Log streaming infrastructure
- `OutputTail` (`output_tail.rs`) - The last lines one server printed, which a
  launch that fails reads for the line that says why (the rule is in its docs)
- `exit_report` (`exit_report.rs`) - What gglib's log says when a server stops: one
  line saying why a stop gglib asked for was asked (`StopReason`), or, for an exit
  nobody asked for, how it ended and the last lines it printed, at warn; an exit
  with 0 or a stop signal is one line, as it is not a failure. A launch watches
  each server it starts, so an exit is told of when it happens
- `check_http_health` / `wait_for_http_health` - Health checks, at each runtime's own path
- `shutdown_child` / `kill_pid` (`shutdown/`) - Stopping a server: SIGTERM, then SIGKILL with a bounded wait

`GuiProcessCore::spawn` ties each server it starts to this process with
`gglib_core::utils::process::tied::tie_to_this_process`. On Windows a
kill-on-close job ends the server when this process ends, however it ends;
on macOS and Linux its process group, the teardown and the startup sweep
stop it.

# Residency and admission

`ProcessManager` routes; [`ResidentSet`] (`residency/`) owns the state. M9
replaced the old single-swap strategy with a bounded resident set — a small
number of VRAM slots filled by an `AdmissionQueue` (`admission/`) that decides,
per request: serve from a resident model, launch into a free or evictable
slot, wait, or give up.

A set may be *pinned*: `ProcessManager::set_pin` restricts admission to
exactly one model (backing `gglib serve`), and admission answers any other
with `PinnedModelMismatch` rather than swapping. The request is resolved first and the pin
compares catalog ids, so the pinned model answers to its id and to its name; a
model the catalog does not hold is `ModelNotFound`, pinned or not. Pinning
changes only which models are admitted — startup coordination, cache handling
and launch options are identical either way.

Every launch surface — the CLI, the proxy, both GUIs — shares one manager
built by `build_service_graph` inside the daemon, which is what makes "gglib
owns every llama-server on this machine" an invariant. A `Concurrent`
strategy existed here for the GUI's earlier direct-spawn path; epic #630
routed the GUI through the proxy's manager instead, so it was deleted along
with the rest of that path.

# Launch options

The `ResidentSet` carries a standing `ServerConfigOptions` template rather
than a hand-picked list of fields. Each launch resolves to

```text
template  ⊕  per-call overrides  ⊕  this request's context chain
```

so a flag added to `ServerConfigOptions` reaches llama-server through this path
with no change here at all.

# On the `Gui` prefix

`GuiProcessCore` was named against a second, port-aligned `ProcessCore` in
`process_core.rs`, which implemented a `ProcessRunner` trait for CLI use with
no log or event infrastructure. `process_core.rs` went in #708; the trait
outlived it and went in #849, once it had no implementor left. This is now the
only process core and serves every caller — the prefix is vestigial.

<!-- module-docs:end -->
