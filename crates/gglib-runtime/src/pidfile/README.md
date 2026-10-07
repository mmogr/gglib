# pidfile

<!-- module-docs:start -->

PID file management for tracking llama-server processes.

Provides atomic I/O, process verification, and startup orphan cleanup.

# Readers
The sweep is one reader. The other is a `gglib` command in a terminal, a
process apart from the one that owns the servers: `list_pidfiles` is how it
learns what is being served under its data root (`RecordedServers` in
`gglib-cli`). To either reader a pid file is a claim to check: one can
outlive its server, so it counts only while `is_our_llama_server` says its
pid is still one.

# Safety guarantees
- Atomic writes via temp file + rename
- Process verification before killing (prevents PID reuse issues)
- Conservative cleanup (if verification fails, only delete PID file)

# Platforms
The sweep verifies an orphan by its executable path: `/proc/<pid>/exe` on
Linux, `sysinfo` on macOS and Windows. On Linux and macOS a verified orphan
gets SIGTERM, then SIGKILL if it is still running 2 seconds later. On Windows
it is force-killed with `taskkill /F`, which gives it no chance to clean up.

<!-- module-docs:end -->
