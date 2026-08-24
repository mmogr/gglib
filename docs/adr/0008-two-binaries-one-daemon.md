# ADR 0008 — Two binaries, one daemon: the CLI and the desktop app stay separate

- **Status:** Accepted
- **Date:** 2026-08-23
- **Depends on:** nothing
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

Every release ships two binaries per platform: `gglib` (the CLI, from
`crates/gglib-cli`) and `gglib-app` (the Tauri desktop app, from `src-tauri`).
The recurring question is whether they should be one, and the recurring answer
has been a shrug. This records the measurements so the question stops being
re-litigated from intuition.

The split is not what it looks like from the outside. It is not "GUI versus
CLI": both are clients of a single daemon, and the daemon is the product.

- `gglib-app` calls `Daemon::connect_or_launch` (`src-tauri/src/daemon/mod.rs`),
  which probes `127.0.0.1:9887`, then spawns a detached `gglib daemon run`, and
  only hosts the composition in-process if no external binary can be found.
- `gglib gui` (`crates/gglib-cli/src/handlers/gui.rs`) launches whichever GUI
  artifact sits beside the running executable.

It is also not a *source* split. `Makefile` builds both binaries in a single
`cargo build --release -p gglib-cli -p gglib-app` invocation, with shared
dependency compilation, under a comment explaining that this is deliberately
how double compilation is avoided. Merging would change the artifact count and
nothing else about how the code is organised.

## Findings

### 1. Windows cannot serve both roles from one executable

`src-tauri/src/main.rs` sets
`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, with a
comment forbidding its removal. A GUI-subsystem PE has no attached console, so
CLI output from a merged binary would go nowhere. The usual workaround,
`AttachConsole(ATTACH_PARENT_PROCESS)`, returns the shell prompt immediately
and interleaves output with it — and here it is worse than merely awkward:
`unsafe_code = "deny"` is set in `[workspace.lints.rust]` and re-declared in
both `crates/gglib-cli/Cargo.toml` and `src-tauri/Cargo.toml`, so a raw FFI call
would need an explicit exemption in a workspace that currently has none.

The inverse — a console-subsystem binary that calls `FreeConsole` when launched
as a GUI — trades the problem for a console window that flashes on every
desktop launch.

This is the one blocker with no acceptable resolution.

### 2. The Linux objection is real but much weaker than it appears

The intuitive argument is that merging drags GTK and WebKit into the CLI and
kills headless use. Two corrections.

`.github/workflows/ci.yml`'s `cli-cross-os` job compiles `gglib-cli` on Linux
with `libasound2-dev` alone and passes. The CLI is not entangled with GTK today.
The GTK/WebKit packages that `release.yml` installed in its CLI-only `build` job
were vestigial, and have been removed.

And "it would not run headless" conflates two things. A dynamically linked
binary fails at load only when the shared objects are *absent*, not when there
is no display: on a headless machine with the libraries installed it execs
fine, and GTK is never initialised until something asks for a window. The real
cost is a large dependency closure in slim containers, which is a reason, not a
blocker.

Worth recording for whoever revisits this: the Linux tray uses `ksni`, a
pure-Rust D-Bus implementation, not `libayatana-appindicator3`. And
`src-tauri/src/tray/layer_shell.rs` already loads `libgtk-layer-shell` at
runtime through `libloading`, so runtime loading is a technique this codebase
uses rather than one it lacks — it simply does not scale to wry's API surface.

### 3. The macOS objection is largely void for this app

The intuitive argument is that the `.app` bundle is the deliverable and running
`Contents/MacOS/gglib-app` directly breaks resources and signing. Neither holds
here. `src-tauri/tauri.conf.json` has `"resources": []` and `"externalBin": []`,
and the frontend is compiled in via `generate_context!()`, so nothing is loaded
bundle-relative. Nothing is signed either: the `macOS` bundle block carries only
`entitlements` and `minimumSystemVersion`, with no `signingIdentity`, and
`scripts/macos-install.command` exists precisely to strip the quarantine
attribute from an unsigned app.

What does break is narrower: no `Info.plist` identity, so a generic icon and no
bundle-keyed TCC state. Note also that the release already ships the bare
binary alongside the `.app`, which contradicts the premise that the bundle is
the only deliverable.

### 4. "Both are clients of the daemon" is true with one exception

`src-tauri/src/commands/llama.rs` calls `gglib_runtime::llama::*` directly in
the desktop process to check for and install the llama.cpp toolchain, streaming
progress over Tauri IPC. The daemon already exposes the same capability over
HTTP (`POST /api/config/system/install-llama`,
`GET /api/config/system/llama-status` in `crates/gglib-axum/src/routes.rs`), and
the frontend has both paths wired — the Tauri one is marked
`TRANSPORT_EXCEPTION` in `src/services/platform/llamaInstall.ts`.

This is recorded rather than fixed here. **Deletion criterion:** when the
desktop install flow moves onto the HTTP route, the direct calls and the
transport exception go with it, and the claim becomes unqualified.

### 5. The desktop app already links the whole backend

Through `gglib-axum`, `gglib-app` depends on `gglib-app-services`, `gglib-db`,
`gglib-mcp`, `gglib-gguf`, `gglib-sse`, `gglib-bootstrap`, `gglib-runtime`, and
transitively the proxy and agent crates. The CLI-exclusive dependencies are
`clap`, `rustyline`, `termimad`, `console` and `mdns-sd`.

So the size argument runs opposite to the intuition: merging would cost the
*CLI* a large GTK/WebKit closure, and cost the *desktop app* almost nothing.
An asymmetric merge — make `gglib-app` the combined binary and keep `gglib`
lean for servers — is therefore the only merge shape that is not obviously
bad, and it remains blocked by finding 1.

## Decision

**1. The two binaries stay separate.** Finding 1 is decisive on its own, and
findings 2 and 3 are costs rather than blockers.

**2. The reasons are recorded at their real strength.** Findings 2 and 3 are
weaker than the arguments previously offered for this split. Anyone revisiting
this should argue against the Windows subsystem constraint, not against a
headless-Linux claim that does not hold.

**3. The split is a packaging concern, so packaging is where the work goes.**
The defects that prompted this were all in packaging and lookup, not in the
binary count: installers built and discarded, `gglib gui` unimplemented on
Windows, the daemon lookup missing `.exe`, and a dashboard resolved against the
working directory.

**4. An asymmetric merge stays open.** If finding 1 is ever resolved, the shape
worth reconsidering is `gglib-app` gaining CLI subcommands while `gglib` stays
lean — not one binary for everything.

## Consequences

**Good:**

- Server installs keep a CLI with no desktop dependency closure.
- Each artifact keeps the packaging that suits it: a tarball on `PATH` for the
  CLI, `.app`/AppImage/deb/rpm/NSIS for the desktop app.
- The daemon stays the single backend, so a fix lands once for both clients.

**Costs, accepted:**

- Two downloads for a desktop user who also wants the CLI. The macOS
  `externalBin` sidecar would reduce this to one and is not yet done.
- Each binary must locate the other on disk, which is a lookup that can be
  wrong per platform — and was, on Windows, in both directions.
- The dashboard is embedded in both binaries, so its bytes ship twice. This is
  deliberate: `gglib-app` hosts the daemon in-process when no external `gglib`
  is found, and that daemon should serve a dashboard.

**Stated plainly, because it surprises people:**

- A debug build of `gglib` is not relocatable. `rust-embed` selects a dynamic
  implementation under `debug_assertions` that reads assets from the absolute
  path recorded at compile time, so a debug binary moved to another machine
  serves no dashboard. Release builds embed the bytes and have no such
  dependency.
