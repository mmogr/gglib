# shutdown

<!-- module-docs:start -->

Graceful process shutdown for llama-server instances.

Provides two shutdown strategies:
- `shutdown_child`: For running processes with a `Child` handle (includes reaping)
- `kill_pid`: For orphaned processes from crashes (no reaping, PID-only)

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`child.rs`](child.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-child-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-child-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-child-coverage.json) |
| [`pid.rs`](pid.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-pid-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-pid-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-shutdown-pid-coverage.json) |
<!-- module-table:end -->

</details>
