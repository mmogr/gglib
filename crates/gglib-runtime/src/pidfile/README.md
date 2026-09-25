# pidfile

<!-- module-docs:start -->

PID file management for tracking llama-server processes.

Provides atomic I/O, process verification, and startup orphan cleanup.

# Safety guarantees
- Atomic writes via temp file + rename
- Process verification before killing (prevents PID reuse issues)
- Conservative cleanup (if verification fails, only delete PID file)

<!-- module-docs:end -->
