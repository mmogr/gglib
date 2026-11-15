# ADR 0029 — CI runs the Rust tests on Linux, macOS and Windows

- **Status:** Accepted
- **Date:** 2026-11-15
- **Depends on:** nothing
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** none

## Context

Until this decision CI ran `cargo test` on Linux only. On macOS and Windows it
compiled `gglib-cli`'s tests and ran clippy, and executed no test binary. The
first run of gglib 0.37.0 on Windows found that a new user could not get from
the installer to a working chat
([#1959](https://github.com/mmogr/gglib/issues/1959): 6 blockers, 9 high and
13 lower findings), none of which CI could have shown.

The first run of the suite on the other two platforms, at commit `bc079293`
([run 43702029039](https://github.com/mmogr/gglib/actions/runs/43702029039),
2026-11-15), read with `scripts/known_failures.py check` against an empty
list:

| | Tests run | Rows that fail |
|---|---|---|
| macOS (`macos-26`, aarch64) | 7,892 | 0 |
| Windows (`windows-2022`, x86_64) | 7,659 | 112 |

Of the 112 on Windows, by the message each test printed:

- 61 start the built `gglib.exe`, which overflows its stack in a debug build
  on every command ([#1969](https://github.com/mmogr/gglib/issues/1969));
- 26 compare a committed file with what the code writes, and failed because
  Git for Windows checked the file out with CRLF;
- 25 are differences in the code under test: the daemon lock's holder, a
  process that is gone read as alive, the tool finder, path forms, files that
  cannot be replaced while open, and others.

No test binary crashed or hung on either. The numbers can be read again from
the run's `test-run-<target>` artifacts for 30 days, and not after.

## Decision

1. **The same suite runs on three platforms.** `ci.yml`'s `test-cross-os`
   builds and runs what `test` does, the whole workspace with its doctests,
   on `macos-26` and `windows-2022`. It runs when `scope` says a change can
   reach a Rust build and after Linux clippy passes, and `CI Success` counts
   it.
2. **Each of the two runs is held to a list of known failures**,
   `scripts/known-failures/macos.txt` and `windows.txt`: one sorted row per
   test, `<package> <target> <test>`. `scripts/known_failures.py` gives the
   job's verdict. A failure that is not listed fails the job. So does a
   listed test that passes, is ignored or is gone, and a test binary that
   printed no result.
3. **The lists only shrink.** The change that makes a listed test pass
   deletes its row. No row is added for a test that passed.
4. **A checkout is the same bytes on every platform.** `.gitattributes` sets
   `* text=auto eol=lf`, so a test that compares a committed file reads what
   Linux reads.
5. **What CI runs where:**

   | | Linux | macOS | Windows |
   |---|---|---|---|
   | Format, repository checks, bindings, rustdoc, frontend | yes | no | no |
   | Clippy, whole workspace | yes | yes | yes |
   | Rust tests and doctests | yes | yes, against its list | yes, against its list |
   | The pinned engines installed, and gglib's launch flags held to them | yes | yes | no |
   | The phone's crates and the Swift framework | no | yes | no |

## Consequences

- A change that breaks a test on macOS or Windows fails its pull request. A
  change that fixes a listed test fails too, until it deletes the row, so a
  list cannot keep a row for a test that passes.
- The Windows leg took 42 minutes with no cache in the first run (34 to build
  the test binaries, 6 to run them), and the macOS leg 20. On `main` the job
  runs only when `Cargo.lock` or the toolchain moves, which is when its cache
  is saved.
- A test binary that crashes or hangs on one platform cannot be listed. It
  fails the job until the test is fixed or gated to the platforms its code
  runs on.
- A row stands for a test by name. It does not record why the test fails,
  and a test that starts failing for a second reason stays listed.

## Kill criteria

- **The lists are empty.** `grep -vc '^#' scripts/known-failures/windows.txt`
  reads 86 on 2026-11-15, and the same command on `macos.txt` reads 0. When
  both read 0, decisions 2 and 3 have done their work: the lists and the
  script's list rules go, and `cargo test`'s own status decides the job.
- **A list grows.** `git log -p -- scripts/known-failures/` shows every row
  ever added. A row added after this decision for a test that used to pass
  means decision 3 is not holding, and the list is then hiding regressions
  where it was meant to count them down.

## Out of scope

- Installing and starting the pinned engines on Windows
  ([#1961](https://github.com/mmogr/gglib/issues/1961)).
- Tests gated to other platforms, which no Windows run executes
  ([#1963](https://github.com/mmogr/gglib/issues/1963)). Windows ran 233
  fewer tests than macOS in the first run.
- The frontend's tests, which run on Linux in one browser engine.
- Running the built installer or a release's binaries.
