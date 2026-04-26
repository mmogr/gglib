# Contributing to gglib

This document is the definitive engineering guide for contributors. Read it before opening a pull request.

---

## Table of Contents

1. [Core Philosophy](#core-philosophy)
2. [Architecture Overview](#architecture-overview)
3. [GUI Parity Principle](#gui-parity-principle)
4. [Concurrency Model](#concurrency-model)
5. [Subprocess Invocation](#subprocess-invocation)
6. [Crate Boundaries](#crate-boundaries)
7. [Documentation Standards](#documentation-standards)
6. [Badges Pipeline](#badges-pipeline)
7. [Development Workflow](#development-workflow)
8. [CI Pipeline](#ci-pipeline)
9. [Pull Request Checklist](#pull-request-checklist)

---

## Core Philosophy

**Small, focused, low-complexity files.** If a module is growing, that is a signal to decompose it, not to add more to it. Functions should do one thing. Files should have one responsibility.

**DRY without ceremony.** When the same logic appears twice, extract it. When extraction requires a new abstraction, make sure that abstraction earns its existence — it should simplify the call sites, not complicate them.

**No backwards compatibility obligation.** If an existing signature, struct layout, or module boundary is in the way of a cleaner design, refactor it. Callers are in the same workspace; update them. There is no public API contract to preserve here.

**Minimum viable surface area.** Do not add configuration knobs, trait objects, or generic parameters for hypothetical future requirements. The right abstraction is the one that solves the problem at hand with the fewest moving parts.

---

## Architecture Overview

The workspace is organized into layers. Dependencies flow strictly inward.

```
┌──────────────────────────────────────────────────────────────┐
│  Surfaces (one per interface)                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  gglib-cli   │  │  gglib-axum  │  │  gglib-tauri │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                  │               │
├─────────▼─────────────────▼──────────────────▼──────────────┤
│  Shared Backend                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ gglib-runtime│  │  gglib-agent │  │  gglib-app-services   │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                  │               │
├─────────▼─────────────────▼──────────────────▼──────────────┤
│  Domain & Infrastructure                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  gglib-core  │  │   gglib-db   │  │  gglib-hf    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└──────────────────────────────────────────────────────────────┘
```

**`gglib-core`** is the pure domain layer: types, traits, error definitions, and path utilities. It has no adapter dependencies and must not acquire any. This is enforced in CI.

**`gglib-runtime`** orchestrates processes (llama.cpp, llama-server). It owns the build and install pipelines.

**Surface crates** (`gglib-cli`, `gglib-axum`, `gglib-tauri`) adapt the shared backend to their output medium. They contain no business logic. Any feature added to one surface must be achievable on all three — see the [GUI Parity Principle](#gui-parity-principle).

---

## GUI Parity Principle

Every capability offered through one interface must be reachable through all three. Downloads, builds, agent loops, and model management all follow the same pattern:

1. **Core logic in a runtime or domain crate** — emits typed events over a `tokio::sync::mpsc::Sender<T>` channel. It has no knowledge of the terminal, HTTP, or Tauri.
2. **Surface adapters consume the channel** — the CLI renders events as an `indicatif` progress bar; the Axum layer streams them as SSE; the Tauri layer emits them as Tauri events to the WebView.

Concrete examples of established patterns:

| Domain | Event type | CLI consumer | Axum consumer | Tauri consumer |
|---|---|---|---|---|
| Agent loop | `AgentEvent` | spinner + streaming print | SSE at `/api/agent/stream` | `agent-event` Tauri event |
| llama install | `LlamaProgressEvent` | progress bar | SSE at `/api/llama/install` | `llama-install-progress` |
| llama build | `BuildEvent` | spinner + progress bar | SSE at `/api/config/system/build-llama-from-source` | `llama-build-progress` |

When adding a new long-running operation:

- Define the event enum in the relevant runtime or domain crate.
- The function signature takes `tx: tokio::sync::mpsc::Sender<YourEvent>` as a parameter.
- Wire the CLI adapter in its own function. Wire the Axum handler. Wire the Tauri command.
- All three ship in the same PR.

**Tauri commands are OS integration only.** Product features are served over HTTP (Axum). The CI enforces that `#[tauri::command]` functions live only in a small set of approved files (`util.rs`, `llama.rs`, `app_logs.rs`). A new product feature does not get a Tauri command — it gets an Axum route that the WebView calls over HTTP, just like the browser-based UI does.

**Frontend transport is unified.** The frontend client modules must not branch on `isTauriApp`. If you find yourself writing `if (isTauriApp()) { invoke(...) } else { fetch(...) }` in a service module, that is an architectural violation. The transport abstraction layer handles that distinction.

---

## Concurrency Model

The codebase uses Tokio for the async runtime. Understanding the boundary between async Tokio tasks and OS threads is critical.

### Subprocess I/O: use `std::thread::spawn`

Reading from a subprocess's stdout or stderr is blocking I/O. This must happen on an OS thread, not a Tokio task.

```rust
// Correct: OS thread reads from subprocess, sends over async channel
let (tx, rx) = tokio::sync::mpsc::channel::<BuildEvent>(64);

let tx_thread = tx.clone();
std::thread::spawn(move || {
    let reader = BufReader::new(child.stdout.take().unwrap());
    for line in reader.lines().map_while(Result::ok) {
        // blocking_send is safe and correct from a std::thread context
        if tx_thread.blocking_send(BuildEvent::Log { message: line }).is_err() {
            break; // receiver dropped, stop reading
        }
    }
});

// Caller drives the Tokio side
while let Some(event) = rx.recv().await {
    // render, forward, emit...
}
```

`blocking_send` is safe to call from a `std::thread` because it is not running on the Tokio executor — there is no risk of stalling async task scheduling. The panic risk from `blocking_send` exists only inside a `tokio::spawn(async { ... })` future, which is why subprocess readers get their own OS threads.

### Do not read subprocess output on the Tokio executor

The following is incorrect:

```rust
// Wrong: blocks the Tokio executor thread
tokio::spawn(async move {
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    while let Some(line) = lines.next_line().await { ... }
});
```

Use `tokio::process::Command` with async I/O, or use `std::thread::spawn` with blocking reads. Choose based on what the rest of the function's call chain expects.

### Channel capacity

All event channels are created with a bounded capacity of 64. This provides backpressure if a consumer falls behind. Do not use unbounded channels for subprocess output.

---

## Subprocess Invocation

When constructing a `std::process::Command` or `tokio::process::Command`, be defensive about the environment it inherits.

### Merging environment variables

Do not blindly set environment variables that may already exist in the caller's environment. For example, if a build step requires `-Wno-missing-noreturn`, do not do this:

```rust
// Wrong: silently discards any CXXFLAGS the user or parent process set
cmd.env("CXXFLAGS", "-Wno-missing-noreturn");
```

Instead, read the existing value and append:

```rust
// Correct: preserves upstream flags
let existing = std::env::var("CXXFLAGS").unwrap_or_default();
let merged = format!("{existing} -Wno-missing-noreturn").trim().to_owned();
cmd.env("CXXFLAGS", merged);
```

The same principle applies to `CFLAGS`, `LDFLAGS`, `CMAKE_ARGS`, and any other flag-aggregating variables. One `.env()` call per variable.

### Capturing output

Subprocesses that produce output must always be spawned with `Stdio::piped()`. Never use `.status()` or `.output()` on a long-running subprocess that would print to the terminal — those methods either inherit the TTY or block until exit, neither of which is compatible with the streaming event model.

```rust
let mut child = Command::new("cmake")
    .args(&["--build", "."])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

---

## Crate Boundaries

The CI runs `scripts/check_boundaries.sh` on every push and pull request. Violations fail the build.

**`gglib-core`** — Pure domain types, error types, and path resolution utilities. No I/O, no async runtime, no adapter crates.

**`gglib-db`** — May depend on `gglib-core` and `sqlx`. Nothing else.

**`gglib-runtime`**, **`gglib-agent`**, **`gglib-download`**, **`gglib-hf`** — May depend on `gglib-core`, `gglib-db`, and peer library crates in the same layer. Must not depend on any surface crate.

**`gglib-app-services`** — Backend bridge used by both `gglib-axum` and `gglib-tauri`. No surface-specific code.

**Surface crates** (`gglib-cli`, `gglib-axum`, `gglib-tauri`) — May depend on anything in lower layers. Must not depend on each other.

If your change requires adding a dependency from a lower layer to a higher layer, reconsider the design. The dependency should flow in the opposite direction via the channel/event pattern described above.

### Feature flags in `gglib-runtime`

`gglib-runtime` uses feature flags to gate compilation of heavy subsystems:

| Feature | Includes | Use in |
|---|---|---|
| *(default)* | Inference and server management | `gglib-axum`, `gglib-app-services` |
| `prebuilt` | Pre-built binary download support | `gglib-tauri`, `gglib-app-services` |
| `cli` | Source build pipeline (`build/`, `install/`) — implies `prebuilt` | `gglib-cli`, any crate that drives source builds |

When adding a new flag-gated import in a surface crate, ensure its `Cargo.toml` declares the correct `features = [...]` value. A missing feature flag will produce a confusing "function not found" compile error rather than a clear feature gate message.

---

## Documentation Standards

This codebase has three distinct documentation surfaces. Each has a defined purpose and a defined location. Understanding the split prevents duplication and keeps the right audience reading the right thing.

### Surface 1: Crate READMEs (shields.io badges + ASCII architecture diagrams)

Each crate's `README.md` serves two narrow purposes:

1. **Badges** — metrics surfaced as shields.io endpoint badges (tests, coverage, LOC, complexity) that read from the `badges` branch (see [Badges Pipeline](#badges-pipeline) below).
2. **Architecture ASCII diagrams** — a text diagram showing where the crate sits in the layer model and an internal structure diagram. These are written and maintained by hand.

Crate READMEs are **not** the place for API documentation, usage examples, or explanatory prose about how individual types work — that belongs in Rustdoc.

### Surface 2: Module-level Rustdoc (`//!`)

Every public module should open with a `//!` block that explains:

- What the module is responsible for.
- What it is **not** responsible for (prevents scope creep in future changes).
- If it is part of a streaming pipeline, a table showing which consumers read from its channel.

```rust
//! Download progress events for the pre-built binary pipeline.
//!
//! [`DownloadEvent`] is produced by [`download_prebuilt_binaries`] and consumed by:
//!
//! | Consumer    | Output                                          |
//! |-------------|--------------------------------------------------|
//! | CLI         | `indicatif` progress bar                        |
//! | Axum        | SSE stream at `GET /api/llama/install`           |
//! | Tauri       | `llama-install-progress` event to WebView        |
```

This is the canonical location for architectural explanation of a module. When you add a new module, write the `//!` block first — it forces you to be clear about what the module owns before you write a single line of implementation.

### Surface 3: Item-level Rustdoc (`///`)

All `pub` types, enums, variants, traits, and functions must have a `///` doc comment. One sentence is enough for simple items; use longer descriptions only when the behaviour is non-obvious.

```rust
/// The build completed successfully.
Complete { version: String, acceleration: String },
```

### The `cargo test --doc` gate

CI runs `cargo test --doc --verbose` on every PR. This compiles all `///` example blocks as Rust code — a dangling import or wrong type in a doc-test will fail CI. When you add a triple-backtick Rust example, make sure it compiles. If an example requires external infrastructure, mark it `no_run`:

````rust
/// ```no_run
/// let events = open_event_stream().await?;
/// # Ok::<(), anyhow::Error>(())
/// ```
````

Private helper functions, unit-test modules (`#[cfg(test)]`), and generated code do not require doc comments.

### Cargo docs deployment

`cargo doc` is deployed to GitHub Pages automatically when a release is published, via `.github/workflows/docs.yml`. It runs:

```bash
cargo doc --workspace --no-deps --document-private-items --exclude gglib-app
```

The published site redirects to `gglib_core/index.html`, which is the primary API reference. You can preview locally with `make doc` (opens the browser). Do not add a docs deployment step manually — the release workflow handles it.

---

## Badges Pipeline

Badges in crate READMEs are **not** static images. They are shields.io endpoint badges that read JSON files from a dedicated `badges` branch. Do not author or edit badge JSON files manually.

### How the pipeline works

```
CI run (ci.yml)
  └─ uploads artifacts: test-results, boundary-status.json, ts-test-results.json
        │
        ▼
badges.yml (triggers after ci.yml completes)
  └─ downloads CI artifacts
  └─ generates badge JSON files (tests, boundaries, TS tests)
  └─ commits JSON to the 'badges' branch

coverage.yml (runs on push to main)
  └─ generates lcov.info via cargo-llvm-cov
  └─ triggers badges.yml (coverage variant)
  └─ per-crate and per-module coverage JSONs pushed to 'badges' branch
```

Shields.io resolves badge URLs like:
```
https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json
```

### Module badge tables in READMEs

Crate READMEs contain per-module badge tables delimited by HTML comment markers:

```html
<!-- module-table:start -->
| Module | Tests | Coverage | LOC | Complexity |
|--------|-------|----------|-----|------------|
| ...    | ...   | ...      | ... | ...        |
<!-- module-table:end -->
```

`scripts/generate_module_tables.sh` regenerates these tables by discovering the actual `.rs` files and subdirectories in each crate and wiring up the corresponding badge URLs. Run it after adding or removing modules:

```bash
./scripts/generate_module_tables.sh           # update all tables in-place
./scripts/generate_module_tables.sh --check   # CI mode: exit 1 if any table is out of date
./scripts/generate_module_tables.sh --dry-run # preview changes without writing
```

The `--check` mode is not currently a CI gate but is intended to become one. Keep tables current when you add modules.

### Adding a badge to a new crate README

Badge URLs follow the pattern `gglib-{crate-name}-{metric}.json` on the `badges` branch. For a new crate `gglib-foo`:

```markdown
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-complexity.json)
```

The badge JSON files will appear on the `badges` branch automatically after the first CI run that includes the new crate. Until then, the badges render as "unknown" — that is expected.

To pre-generate badge structure for a new crate or update module tables, use `scripts/generate_submodule_readmes.sh`. This script updates existing README files with the standard badge block and module table markers; it does not create new README files.

```bash
./scripts/generate_submodule_readmes.sh           # update all existing READMEs
./scripts/generate_submodule_readmes.sh --dry-run # preview changes
```

---

## Development Workflow

### Prerequisites

- Rust 1.91.0 (managed via `rust-toolchain.toml` — `rustup` will install it automatically)
- Node.js 20+
- Platform system libraries (see `scripts/check-deps.sh` for a live dependency check)

Run `make setup` for a one-command first-time setup on macOS. On Linux, review `scripts/check-deps.sh` first to install system packages.

### Common commands

```bash
# Compile-check without producing artefacts (fastest feedback loop)
make check

# Run all tests
make test

# Format code (must be clean before commit)
make fmt

# Run Clippy — treat all warnings as errors
make lint

# Build and open Rustdoc locally
make doc

# Run all pre-commit checks in sequence: fmt + lint + check + test
make pre-commit
```

### Working on the frontend

```bash
npm install
npm run dev          # Start Vite dev server
npm run test:run     # Run Vitest suite
npm run build        # Production build (required before integration tests)
```

### Testing with feature flags

Some crates have conditional compilation gated on feature flags. A plain `cargo test` will use default features. To test a specific feature combination:

```bash
cargo test -p gglib-runtime --features cli
cargo doc  -p gglib-runtime --features cli
```

### Lockfile discipline

The Cargo lockfile (`Cargo.lock`) is committed and must stay consistent. CI runs `cargo metadata --locked` as an early gate. After editing any `Cargo.toml`, run `cargo generate-lockfile` and commit the result.

---

## CI Pipeline

Every PR must pass the following gates in order. They are not advisory.

| Gate | Command | What it enforces |
|---|---|---|
| **Format** | `cargo fmt --all -- --check` | Consistent code style |
| **Boundaries** | `./scripts/check_boundaries.sh` | Layer dependency rules |
| **Architecture** | `./scripts/check-tauri-commands.sh`, `check-frontend-ipc.sh`, `check_transport_branching.sh` | Tauri policy; no IPC in product routes; no frontend transport branching |
| **Clippy** | `cargo clippy --all-targets --all-features -- -D warnings` | No warnings, ever |
| **Rust tests** | `cargo test` (aggregate + per-crate) | Correctness |
| **Doc tests** | `cargo test --doc --verbose` | Doc examples compile and run |
| **Frontend tests** | `npm run test:run` | TypeScript correctness |
| **Cross-OS check** | `cargo check` on Linux/macOS/Windows | No platform-specific breakage |

After each successful CI run, `badges.yml` downloads the test/boundary/coverage artifacts and pushes updated badge JSON files to the `badges` branch. Shields.io badges in crate READMEs resolve from there.

Coverage is measured on every push to `main` with `cargo-llvm-cov` and feeds into the same badge pipeline.

Docs are deployed to GitHub Pages automatically when a release is published, via `docs.yml`.

---

## Pull Request Checklist

Before requesting review, confirm each item:

- [ ] `make pre-commit` passes locally (`fmt` + `lint` + `check` + `test`).
- [ ] `cargo test --doc` passes.
- [ ] Any new public type or enum has `///` doc comments on all items.
- [ ] Any architectural change is documented in `//!` module-level Rustdoc. ASCII architecture diagrams belong in crate READMEs; prose API documentation does not.
- [ ] If a new module was added, `./scripts/generate_module_tables.sh` has been run and the updated badge table is committed.
- [ ] Subprocess I/O is captured with `Stdio::piped()` and read on an OS thread, not a Tokio task.
- [ ] Environment variable merging uses read-then-append, not a bare `.env()` that overwrites.
- [ ] Any feature gated behind `#[cfg(feature = "...")]` is declared correctly in all consuming `Cargo.toml` files.
- [ ] If the change adds a new long-running operation, all three surfaces (CLI, Axum, Tauri) are wired up.
- [ ] `Cargo.lock` is up to date and committed.
- [ ] No new dependency has been introduced from a higher layer to a lower layer.
