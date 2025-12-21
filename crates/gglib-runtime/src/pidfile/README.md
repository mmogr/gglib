# pidfile

<!-- module-docs:start -->

PID file management for tracking llama-server processes.

Provides atomic I/O, process verification, and startup orphan cleanup.

## Safety Guarantees

- **Atomic writes** — Via temp file + rename
- **Process verification** — Before killing (prevents PID reuse issues)
- **Conservative cleanup** — If verification fails, only delete PID file

## Key Functions

| Function | Description |
|----------|-------------|
| `write_pidfile()` | Atomically write PID file |
| `read_pidfile()` | Read and parse PID file data |
| `delete_pidfile()` | Remove PID file |
| `cleanup_orphaned_servers()` | Kill orphaned processes on startup |

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`io.rs`](io.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-io-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-io-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-io-coverage.json) |
| [`sweep.rs`](sweep.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-sweep-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-sweep-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-sweep-coverage.json) |
| [`verify.rs`](verify.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-verify-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-verify-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-verify-coverage.json) |
<!-- module-table:end -->

</details>
