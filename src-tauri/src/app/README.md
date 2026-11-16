<!-- module-docs:start -->

Application state and shared utilities for the Tauri app.

`AppState` needs nothing a window or the daemon provides, and is managed on
the builder before `build` creates the windows, so no command can find it
missing. What is not known at that point, the daemon, is asked through
`daemon.service`, which waits for the launch.

<!-- module-docs:end -->
