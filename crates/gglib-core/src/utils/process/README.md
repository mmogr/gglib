# process

<!-- module-docs:start -->

Starting other programs, the same way on every OS.

On Windows, a console program started with `std::process::Command::new`
shares its parent's console, or opens a console window of its own when the
parent has none, as the desktop app does in a release build. The
`windows_subsystem = "windows"` attribute on the main binary only suppresses
the window for the *main* process — not any child processes.
`CREATE_NO_WINDOW` gives the child a console of its own with no window.

Use [`cmd`] and [`async_cmd`] instead of `Command::new` at every call site.
The Windows-specific flag is applied here and nowhere else.

- [`host`]: what a gglib binary does to its own process before anything else.
- [`bounded`]: running a program that should answer and exit, with a time
  limit, and why a run failed (`failure_kind`): the one place that reads
  each OS's report of a library that cannot be loaded (Windows' exit
  status, glibc's loader, macOS's dyld), as values, so all three are
  tested on any OS.
- [`detached`]: starting the service, so it outlives the command that
  started it.
- [`stop`]: hearing that this process is asked to stop, and whether it has
  time for an orderly teardown or seconds before the OS ends it.

<!-- module-docs:end -->
