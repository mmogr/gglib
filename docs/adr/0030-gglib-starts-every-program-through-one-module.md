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

- **A program started outside the module.** On 2026-11-16,
  `rg -c 'disallowed_methods' crates src-tauri -g '*.rs'` reads 8. Two are
  `gglib-test-http`'s own, for its rule on accepting connections; the six
  for this rule are two in `utils/process/mod.rs`, one in `detached.rs` and
  three in `gglib-test-proc`. A higher count means the rule is being excused
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
- Tying the engines to the service on Windows, so they end with it
  ([#1985](https://github.com/mmogr/gglib/issues/1985)).
- Reporting a missing Visual C++ runtime, and repairing it (stack 3 of #1959).
