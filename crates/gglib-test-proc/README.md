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

A leaf, taken only from `[dev-dependencies]`, with no dependencies of its own.
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
  │ dir/sd-server[.exe]: reads dir/sd-server.proc, line by line   │
  │ argv <path> · print <text> · eprint <text> · sleep <s> · exit │
  └──────────────────────────────────────────────────────────────┘
```

## Internal Structure

```text
┌───────────────────────────────────────────────────────────────────┐
│                          gglib-test-proc                           │
├───────────────────────────────────────────────────────────────────┤
│  lib.rs    install · instructions_path · parse · Instruction        │
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
by spaces.
