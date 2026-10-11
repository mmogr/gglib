# runs

<!-- module-docs:start -->

A run is one reply the daemon owns from start to end, so it survives the
client that asked for it leaving. `RunRegistry` holds them in memory only —
at most 32, each log at most 8 MB — and serves them through
`gglib_core::ports::RunsPort`, which decides what each scope may see.

# Module Layout

```text
runs/
  mod.rs          — the clock type
  registry.rs     — RunRegistry: create, list, get, events, cancel, the
                    scope rule (a run on a hub chat is the chat's, read by
                    this machine and every device), the limits, retention,
                    forget_device
  admit.rs        — a run admitted under its id, and driven to its end; a
                    panic ends it `failed`; until its end is handled (an
                    agent run's reply saved) every reader sees `in_progress`
                    and no other run is admitted to its conversation, by
                    `RunInfo::holds` on the run as it is shown
  local.rs        — a run the daemon starts with work it prepared (an
                    agent run): reserve in a scope, then start; never on
                    the port
  cell.rs         — one run: status, log, the watch its readers wait on,
                    and its latest preview frame, kept beside the log and
                    never in it, forgotten when its call completes or the
                    run ends
  reader.rs       — a run's event stream: the log by index, then live; the
                    preview only to a reader that has caught up, once per
                    frame, with no seq
  executor.rs     — the seam an executor writes a run's events through
  chat.rs         — the chat executor: the body to the daemon's own proxy,
                    each `data:` payload logged verbatim, `[DONE]` not;
                    `gglib_core::sse::DataFrames` cuts the reply into those
                    payloads, holding no more of one event than the log
                    could take
  door.rs         — where that proxy is and the key it wants, by the
                    tunnel's rule in `remote/key.rs`; never minted
```

# Privacy

A frame's content and a request body never reach a log line, a tracing
field, an error message or the disk. Log ids, statuses, counts and sizes.

<!-- module-docs:end -->
