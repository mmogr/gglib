# Daemon

<!-- module-docs:start -->

Handlers for the `gglib daemon` command group.

`run` hosts [`gglib_axum::run_daemon`] in the foreground — the same
composition every auto-launched daemon runs, on `127.0.0.1` only. `status`
combines the health probe, the lock file's recorded holder, and the proxy
status into one report. `stop` asks the daemon to shut down over the API and
waits for it to actually go away.

The local `stop` is also what the Windows installer and uninstaller run
before they replace or remove the installed `gglib.exe`
(`src-tauri/windows/installer-hooks.nsh`), so it runs before logging and the
library are set up (`dispatch::leaves_nothing_behind`) and asks nothing. With
no daemon answering it touches nothing on disk. With one, it reads the
daemon's token and lock from the data folder its own environment names and
writes nothing; that folder exists unless the daemon was started with
another `GGLIB_DATA_DIR`, and then resolving the default one makes it.
Because it runs before logging is set up, a local `stop` writes nothing to
gglib's log file, and `gglib -v daemon stop` logs nothing more: what it says
is the lines it prints. `stop --remote` goes the ordinary way, with logging.

This module owns presentation only; the daemon's behaviour — lock, sweep,
signals, teardown — lives in `gglib-axum`'s `daemon` module.

<!-- module-docs:end -->
