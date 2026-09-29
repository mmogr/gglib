# run

<!-- module-docs:start -->

`gglib run`: start a reply the daemon finishes, and read it later. A thin
client over `/api/runs/*`; the runs themselves live in the daemon, so closing
the terminal leaves the reply running.

# Module Layout

```text
run/
  mod.rs    — the subcommands, and routing them
  start.rs  — `start`: mint an id, start the run, print the id, or with
              `--follow` the reply as it arrives
  read.rs   — `list`, `show [--follow]`, `cancel`
  text.rs   — the reply text in a frame, from its content delta
```

`show` without `--follow` prints the reply up to the event the run had
reached when asked, and says whether it is still going. A run a paired device
started is listed and can be cancelled here, but its reply is the device's.

<!-- module-docs:end -->
