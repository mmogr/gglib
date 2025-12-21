# shutdown

<!-- module-docs:start -->

Graceful process shutdown for llama-server instances.

Provides two shutdown strategies:

| Strategy | Use Case |
|----------|----------|
| `shutdown_child()` | Running processes with a `Child` handle (includes reaping) |
| `kill_pid()` | Orphaned processes from crashes (no reaping, PID-only) |

## Shutdown Flow

1. Send SIGTERM
2. Wait for graceful shutdown (timeout)
3. Send SIGKILL if still running
4. Reap child process (for `shutdown_child`)

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`child.rs`](child) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-child-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-child-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-child-coverage.json) |
| [`pid.rs`](pid) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-pid-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-pid-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-pid-coverage.json) |
<!-- module-table:end -->

</details>
