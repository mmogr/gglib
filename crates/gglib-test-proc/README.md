# gglib-test-proc

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-proc-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-proc-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-proc-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-test-proc-complexity.json)

Layer: **utility** · [workspace diagram](../README.md#architecture-overview)
<!-- crate-header:end -->

A stand-in program for a test that needs a child process, the same on every
OS. A test that wrote a `#!/bin/sh` script, or started `sh` or `sleep`, could
run only on Unix; this program runs on Windows too, so the code it stands in
for is tested there as well.

## Architecture

A leaf with no dependencies of its own, taken from `[dev-dependencies]`, and
by `gglib-mcp`'s `test-utils` feature, whose stand-in MCP server is this
program and which other crates turn on from theirs.
The library installs the program; the binary is the program.

```text
  a test                          gglib-test-proc
  ┌─────────────────────────┐     ┌────────────────────────────────────────┐
  │ install(dir, "sd-server",│ ──▶ │ builds the program once (cargo build)  │
  │   "argv …\nsleep 60")    │     │ copies it to dir/sd-server[.exe]       │
  │                          │ ◀── │ writes dir/sd-server.proc              │
  │ spawns dir/sd-server     │     └────────────────────────────────────────┘
  └─────────────────────────┘
              │ runs
              ▼
  ┌──────────────────────────────────────────────────────────────┐
  │ dir/sd-server[.exe]: reads dir/sd-server.proc, line by line  │
  │ argv <path> · calls <path> · pid <path> · touch <path>       │
  │ read · print <text> · eprint <text> · sleep <s> · exit <code>│
  └──────────────────────────────────────────────────────────────┘
```

## Internal Structure

```text
┌───────────────────────────────────────────────────────────────────┐
│                          gglib-test-proc                           │
├───────────────────────────────────────────────────────────────────┤
│  lib.rs    install · wrap · instructions_path · parse · Instruction │
│  main.rs   the program: follows the instruction file beside it      │
└───────────────────────────────────────────────────────────────────┘
```

- [`src/lib.rs`](src/lib.rs): `install` builds the program on first use, with
  the cargo running the tests, copies it into place (never a link: each
  copy must know its own path), runs the copy once and writes its
  instructions;
  `instructions_path` is the name rule the program reads its file by; `parse`
  reads an instruction file.
- [`src/main.rs`](src/main.rs): the program. It finds its file from its own
  path, so any number of programs can sit in one directory under different
  names.

## Usage

```rust,ignore
let dir = tempfile::tempdir()?;
let argv = dir.path().join("sd-server.argv");
let sd = gglib_test_proc::install(
    dir.path(),
    "sd-server",
    &format!("argv {}\nsleep 60", argv.display()),
);
// `sd` is dir/sd-server, or dir/sd-server.exe on Windows.
```

In `print` and `eprint`, `{args}` stands for the program's arguments, joined
by spaces. `argv <path>` writes the arguments to `path`, one per line, so the
file holds the last call. `calls <path>` adds one line to the end of `path`,
the arguments joined by spaces, so the file holds every call in the order
they were made. `pid <path>` writes the program's own process id to `path`,
`touch <path>` writes an empty file there, and `read` reads stdin until it
ends.

`wrap(dir, name, program)` writes a script that starts `program` and waits
for it, as a shim such as `npx.cmd` does: `name.cmd` on Windows, run by
`cmd.exe`, and an executable `sh` script elsewhere. A test of what stopping
a shim leaves behind starts the wrapper and finds the program by its `pid`
file. The `sh` script is written by a child `sh`, so no descriptor to it is
ever open in the test's process: Linux refuses to start a file a process
holds open for writing ("text file busy"), and a process another test was
just starting could have held a copy of one. `install` instead runs each new
copy once, which waits out any such copy before the copy is handed back.
