# shutdown

<!-- module-docs:start -->

Graceful process shutdown for llama-server instances.

Provides two shutdown strategies:
- `shutdown_child`: For running processes with a `Child` handle (SIGTERM → SIGKILL escalation with bounded timeout)
- `kill_pid`: For orphaned processes from crashes (no reaping, PID-only)

The bounded post-SIGKILL wait guards against D-state hangs (e.g., blocked CUDA driver ioctls).
If the timeout expires, cleanup proceeds and Tokio's background reaper eventually collects the zombie.

<!-- module-docs:end -->
