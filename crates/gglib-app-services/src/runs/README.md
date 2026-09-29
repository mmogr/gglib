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
                    scope rule, the limits, retention, forget_device
  cell.rs         — one run: status, log, the watch its readers wait on
  reader.rs       — a run's event stream: the log by index, then live
  executor.rs     — the seam an executor writes a run's events through
  chat.rs         — the chat executor: the body to the daemon's own proxy,
                    each `data:` payload logged verbatim, `[DONE]` not
  door.rs         — where that proxy is and the key it wants, by the
                    tunnel's rule in `remote/key.rs`; never minted
  sse.rs          — `data:` payloads out of a byte stream
```

# Privacy

A frame's content and a request body never reach a log line, a tracing
field, an error message or the disk. Log ids, statuses, counts and sizes.

<!-- module-docs:end -->
