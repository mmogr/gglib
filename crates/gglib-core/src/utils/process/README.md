# process

<!-- module-docs:start -->

Starting other programs, the same way on every OS.

On Windows, every child process created with `std::process::Command::new`
inherits a new console window unless `CREATE_NO_WINDOW` is explicitly set.
The `windows_subsystem = "windows"` attribute on the main binary only
suppresses the window for the *main* process — not any child processes.

Use [`cmd`] and [`async_cmd`] instead of `Command::new` at every call site.
The Windows-specific flag is applied here and nowhere else.

- [`host`]: what a gglib binary does to its own process before anything else.
- [`detached`]: starting the service, so it outlives the command that
  started it.

<!-- module-docs:end -->
