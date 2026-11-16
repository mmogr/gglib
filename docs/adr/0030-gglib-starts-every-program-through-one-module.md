# ADR 0030 — gglib starts every program through one module

- **Status:** Accepted
- **Date:** 2026-11-16
- **Depends on:** [ADR 0008](0008-two-binaries-one-daemon.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** none

## Context

The first run of gglib 0.37.0 on Windows
([#1959](https://github.com/mmogr/gglib/issues/1959), on a Windows 11 VM)
found three faults in how the service is started:

- **A4.** The service ran in a visible terminal that opened in front of the
  app. It shared the console of the terminal that started it, so a Ctrl-C
  there on any command (tested with `ping`) stopped it, and closing that
  terminal killed it with no teardown and left llama-server holding 4.5 GB.
- **A5.** A caller that captured the output of a command that starts the
  service, such as `gglib up -y`, waited until the service and its model
  process exited. Windows hands a new process every inheritable handle of
  its parent, and the service took the caller's pipe with them.
- **A1, in part.** An engine missing a DLL put a modal dialog on the screen
  at every probe.

The service was started by two copies of the same code,
`src-tauri/src/daemon/launch.rs` and `crates/gglib-cli/src/daemon_client/mod.rs`.
Neither used `gglib_core::utils::process::cmd`, the helper that already set
`CREATE_NO_WINDOW` on Windows, and nothing stopped them: 50 calls in the
workspace made a `Command` directly (`git grep -n 'Command::new' 18d5bfd6 -- '*.rs'`
outside `utils/process/` reads 52, of which one is clap's `Command` and one a
comment).

## Decision

1. **`gglib_core::utils::process` is the one place a program is started.**
   `cmd` and `async_cmd` make every `Command`; `detached::spawn_daemon`
   starts the service, for the CLI and the desktop app alike. `clippy.toml`
   refuses `Command::new` (std and tokio), `creation_flags` and
   `process_group` everywhere else. 44 of the 50 calls go through `cmd` or
   `async_cmd`. `gglib-test-proc`, which may have no dependencies, keeps the
   other six, under three exceptions that give the reason.
2. **The service is kept apart from the terminal that started it.** On macOS
   and Linux it gets a process group of its own. On Windows
   `CREATE_NO_WINDOW` gives it a console of its own with no window, so the
   starting terminal's Ctrl-C and close button do not reach it.
3. **A gglib process keeps its standard handles private and shows no error
   dialogs.** `host::prepare()`, called first in both `main`s, clears the
   inherit flag on the three standard handles and adds
   `SEM_FAILCRITICALERRORS`, `SEM_NOGPFAULTERRORBOX` and
   `SEM_NOOPENFILEERRORBOX` to the error mode. Every process gglib starts
   inherits the error mode. The handles stay private in the gglib process
   that called `prepare()`; a child given a handle on purpose still gets it,
   because `Stdio::inherit()` passes a copy made for that child. On macOS and
   Linux `prepare()` does nothing; Rust opens every descriptor close-on-exec.
4. **Windows is called directly from `host.rs` only.** The workspace denies
   `unsafe_code`, and the modules that allow it each own one foreign
   interface: llama.cpp in `gglib-engine-llama/src/sys`, gtk-layer-shell in
   the Linux tray; `gglib-core/src/paths/test_utils.rs` allows it for
   `env::set_var` in test set-up. `GetStdHandle`, `SetHandleInformation` and `SetErrorMode`
   have no safe form on the pinned toolchain, so the Windows half of
   `host.rs` allows it too, with a `SAFETY` note on each call. A later
   Windows call that needs `unsafe`, such as the job object of
   [#1985](https://github.com/mmogr/gglib/issues/1985), goes beside them.

> **Amendment, 2026-11-16 ([#1983](https://github.com/mmogr/gglib/issues/1983)).**
> Decision 2 covers a service the CLI or the app starts. A service run in a
> terminal, `gglib daemon run`, shares that terminal's console, and closing
> its window used to end it at once, with no teardown and llama-server left
> running. `gglib_core::utils::process::stop::stop_requested` now hears every
> stop and says how long it leaves: Ctrl-C, SIGTERM and Ctrl-Break are
> orderly, and the teardown runs as before; a console window closed while
> the service serves is hurried, because Windows ends the process about five
> seconds later, so the service kills every engine (llama-server and
> `sd-server`) at once, then runs the same teardown for as long as it lives.
> A window closed during the teardown of an earlier stop still ends the
> process at once. A logoff and a system
> shutdown are listened for as hurried too, but Windows does not send them
> to the service: a console program that loads `user32.dll`, as gglib does,
> is not sent `CTRL_LOGOFF_EVENT` or `CTRL_SHUTDOWN_EVENT`. Hearing those
> would take a hidden window and its `WM_ENDSESSION`, which is not done. A
> test cannot raise the console events; a run on the Windows VM is their
> proof. The engines' kill is tested on every OS.

> **Amendment, 2026-11-16 ([#1985](https://github.com/mmogr/gglib/issues/1985)).**
> On Windows the engines end with the process that started them, however
> it ends: `taskkill /F`, a crash, a console window closed during a
> teardown (which still ends the service at once), a logoff. The owner call
> is `gglib_core::utils::process::tied::tie_to_this_process`, made after
> each llama-server and `sd-server` is started (and after each MCP server,
> until the next amendment started those as trees). On Windows it
> puts the child in a job object the process makes on first use with
> `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and never closes; the handle cannot
> be inherited, so when the process ends Windows closes the last handle and
> ends every process in the job. The calls, `CreateJobObjectW`,
> `SetInformationJobObject` and `AssignProcessToJobObject`, are in
> `host.rs` beside the others. A child already in a job (Windows' SSH
> server and some terminals run their programs in one) is nested, which
> Windows allows since Windows 8; if the assignment fails anyway, the child
> runs on untied, a warning is logged, and nothing else changes. On macOS
> and Linux the call ties nothing and says so (`Tied::NotTied`): the
> engines keep their process groups, the teardown and the orphan sweep at
> the next start, by the owner's decision. A child is tied just after it
> starts, so what it starts in that moment is not; the engines start
> nothing.
> `crates/gglib-core/tests/tied_child.rs` kills a stand-in service outright
> and requires its two tied children gone within 2 s where the call says
> `Tied::ByJob`.

> **Amendment, 2026-11-16 ([#1959](https://github.com/mmogr/gglib/issues/1959), B4).**
> Decision 4 now holds the registry too. The desktop app's Windows installer
> runs the `gglib.exe` it installs, `gglib config path-entry add`, to put
> its folder on the user's own `Path` (`HKEY_CURRENT_USER\Environment`), and
> the uninstaller runs `remove`, which takes off only a folder `add` put on,
> by a record in `HKEY_CURRENT_USER\Software\gglib` (`PathEntriesAdded`).
> `host::Registry`, the `paths::user_path::Store` that gglib-core alone can
> reach, reads and writes those values with `RegGetValueW` (`RRF_NOEXPAND`,
> so `%VAR%` stays unexpanded), `RegSetKeyValueW` and `RegDeleteKeyValueW`,
> and announces a `Path` change with `SendMessageTimeoutW(HWND_BROADCAST,
> WM_SETTINGCHANGE, "Environment")`; off Windows the variable is
> `UserPath::NotKept` and nothing is written. The calls are in `host.rs`
> beside the others, not in the
> installer, because an NSIS string holds 1024 characters in the build Tauri
> uses, and a longer `Path` read into one and written back would be cut
> short. What the edit does to the text is `gglib_core::paths::user_path`,
> tested on every OS.

> **Amendment, 2026-11-16 ([#2012](https://github.com/mmogr/gglib/issues/2012)).**
> An MCP server whose command is a shim is more than one process (`npx.cmd`
> is `cmd.exe` running `node.exe`; a wrapper script is `sh` running its
> program), and stopping the server ended only the first. An MCP server is
> now started with `gglib_core::utils::process::tree::spawn`, a child
> stopped with everything it started. On Windows the child is started
> suspended (`CREATE_SUSPENDED`), put in a kill-on-close job object of its
> own, then resumed, so nothing it starts escapes the job; stopping it calls
> `TerminateJobObject`. Only gglib holds the job's handle, so the tree also
> ends when gglib ends, however it ends, which is what
> `tie_to_this_process` gave MCP servers before. If the job cannot be made
> the child runs on, a warning is logged and stopping ends the child alone;
> if it cannot be resumed it is ended and the start fails. The calls
> (`CreateJobObjectW`, `SetInformationJobObject`,
> `AssignProcessToJobObject`, `TerminateJobObject`,
> `CreateToolhelp32Snapshot`, `Thread32First`, `Thread32Next`, `OpenThread`,
> `ResumeThread`) are in `host.rs`, in a section of their own. On macOS and
> Linux the child leads a process group of its own and stopping sends
> `SIGKILL` to the group while the child is unreaped. A server in its own
> group no longer gets a signal sent to gglib's group (the terminal's
> Ctrl-C or hangup, `kill -9 %1`), so the daemon's teardown now stops
> every MCP server explicitly, and `gglib mcp test` stops the server it is
> testing when it hears a stop. A stop other than a `Drop` follows the MCP
> stdio transport: it closes the server's input, gives it
> `gglib_mcp::SERVER_EXIT_GRACE` (1 s) to exit by itself, writing what it
> keeps, and only then ends the tree; every server is stopped together. Nothing ties a tree to gglib on
> macOS and Linux, as nothing ties the engines. Each test starts a stand-in
> through a shim: `crates/gglib-core/tests/stopped_with_what_it_started.rs`
> (the tree, ten starts and stops), `gglib-mcp`'s `client_stop_tests.rs`
> (the client) and `gglib-axum`'s `teardown_stops_every_mcp_server_and_what_it_started`.

## Consequences

- `crates/gglib-core/tests/detached_spawn.rs` holds the A5 fault on every
  platform: a starter whose output the test reads starts a stand-in service
  that lives a minute, and the read must end within 20 s.
- A new call that starts a program fails clippy on every platform until it
  goes through the module, so the Windows flag cannot be missed again.
- Only the three standard handles are kept from the service. Another handle
  that a caller let gglib inherit still reaches it: std 1.97 always passes
  every inheritable handle to `CreateProcessW`, and a handle list
  (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`) is not stable on `Command`. A caller
  whose own output is captured and that runs `gglib up -y` with an
  inheritable copy of it would still wait for the service. No such caller is
  known; closing it would mean starting the service with `CreateProcessW` in
  `host.rs`.
- No dialog is shown for a missing DLL or a crash in gglib or its engines.
  Each is an exit code, which gglib must report itself (stack 3 of #1959).
- The service no longer dies with the terminal that started it, on Windows
  as on Unix. It stops through `gglib daemon stop`, the app, or a system
  shutdown; how it handles each of those on Windows is #1983.

## Kill criteria

- **A program started outside the module.** On 2026-11-16, after #2012,
  `rg -c 'disallowed_methods' crates src-tauri -g '*.rs'` counts 12 in nine
  files. Two are `gglib-test-http`'s own, for its rule on accepting
  connections, and one is `paths/canonical.rs`'s, for the canonicalize
  rule; the nine for this rule are two in `utils/process/mod.rs`, two in
  `detached.rs`, one in `tree.rs`, one in `host.rs` and three in
  `gglib-test-proc`. A higher count means the rule is being excused
  rather than followed, and the exceptions are reviewed against decision 1.
- **Windows called from a second place.**
  `rg -l 'windows_sys' crates src-tauri -g '*.rs'` reads one file,
  `crates/gglib-core/src/utils/process/host.rs`. Another file means
  decision 4 no longer holds.
- **The fault returns.** `detached_spawn` failing on the Windows leg of
  `test-cross-os`.

## Out of scope

- How the service stops on Windows when the user logs off or the system
  shuts down: Windows does not send those events to it (see the amendment
  under Decision for the console closing, done in
  [#1983](https://github.com/mmogr/gglib/issues/1983)).
- Time limits on the programs gglib waits for
  ([#1984](https://github.com/mmogr/gglib/issues/1984)).
- A service started where its starter's job ends it: Windows' SSH server
  ends every process of a session when the session closes, the service
  included, and its engines with it. Outliving the starter would need the
  starter's job to allow breakaway.
- Reporting a missing Visual C++ runtime, and repairing it (stack 3 of #1959).
