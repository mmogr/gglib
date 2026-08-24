# Contributing to gglib

This document is the definitive engineering guide for contributors. Read it before opening a pull request.

---

## Table of Contents

1. [Core Philosophy](#core-philosophy)
2. [Architecture Overview](#architecture-overview)
3. [GUI Parity Principle](#gui-parity-principle)
4. [UI Conventions](#ui-conventions)
5. [Model Architecture Registry](#model-architecture-registry)
6. [Concurrency Model](#concurrency-model)
7. [Subprocess Invocation](#subprocess-invocation)
8. [Crate Boundaries](#crate-boundaries)
9. [Documentation Standards](#documentation-standards)
10. [Architecture Decision Records](#architecture-decision-records)
11. [Badges Pipeline](#badges-pipeline)
12. [Development Workflow](#development-workflow)
13. [CI Pipeline](#ci-pipeline)
14. [Issue & PR Labeling](#issue--pr-labeling)
15. [Pull Request Checklist](#pull-request-checklist)

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

**Surface crates** (`gglib-cli`, `gglib-axum`, `gglib-tauri`) adapt the shared backend to their output medium. They contain no business logic. Any feature added to one surface must be achievable on all three; how quickly the other surfaces must follow depends on the capability's tier — see the [GUI Parity Principle](#gui-parity-principle).

---

## GUI Parity Principle

Parity is tiered. Every capability must remain *achievable* on all three surfaces (the shared backend guarantees that), but when the other surfaces ship depends on what kind of capability it is:

- **Tier 1 — runtime behaviour.** Anything that changes what the proxy or runtime *does*: request handling, model lifecycle, launch flags, admission, security posture. A Tier 1 capability must be reachable through all three surfaces, and all three ship in the same PR.
- **Tier 2 — management and inspection.** Operator conveniences for looking at or arranging things (e.g. `gglib model explain`, `gglib up`, documentation). These may land CLI-first, provided the PR says so and links a tracked issue for the surface gap — parity debt must be explicit, never silent.

State the tier in the PR description (recent PRs also carry it in the commit message). When in doubt, treat it as Tier 1.

The event-channel pattern below is what makes Tier 1 parity cheap — use it for any long-running operation regardless of tier. Downloads, builds, agent loops, and model management all follow the same pattern:

1. **Core logic in a runtime or domain crate** — emits typed events over a `tokio::sync::mpsc::Sender<T>` channel. It has no knowledge of the terminal, HTTP, or Tauri.
2. **Surface adapters consume the channel** — the CLI renders events as an `indicatif` progress bar; the Axum layer streams them as SSE; the Tauri layer emits them as Tauri events to the WebView.

Concrete examples of established patterns:

| Domain | Event type | CLI consumer | Axum consumer | Tauri consumer |
|---|---|---|---|---|
| Agent loop | `AgentEvent` | spinner + streaming print | SSE at `POST /api/agent/chat` | same SSE stream — no Tauri event |
| llama install | `LlamaProgressEvent` | spinner + progress bar via `consume_install_events_cli` | SSE at `POST /api/config/system/install-llama` | `llama-install-progress` |
| llama build | `BuildEvent` | spinner + progress bar | none — #834 removed the route as dead | none — removed with it |

Every row is a claim about code that exists. The `llama install` row was not
one for a long time: the event type was declared private inside `gglib-axum`,
the runtime took a `Box<dyn Fn(u64, u64)>` that could not express a phase, and
the CLI consumer named here had never been written. Three surfaces each
invented their own statuses on top of two byte counts, which is what a missing
event type costs. Check a row before you copy it as precedent.

When adding a new long-running operation:

- Define the event enum in the relevant runtime or domain crate.
- The function signature takes `tx: tokio::sync::mpsc::Sender<YourEvent>` as a parameter.
- Wire the CLI adapter in its own function. Wire the Axum handler. Wire the Tauri command.
- Tier 1: all three ship in the same PR. Tier 2: the CLI ships now and the remaining surfaces are tracked in a linked issue.

**Tauri commands are OS integration only.** Product features are served over HTTP (Axum). The CI enforces that `#[tauri::command]` functions live only in a small set of approved files (`util.rs`, `llama.rs`, `app_logs.rs`). A new product feature does not get a Tauri command — it gets an Axum route that the WebView calls over HTTP, just like the browser-based UI does.

**Frontend transport is unified.** The frontend client modules must not branch on `isTauriApp`. If you find yourself writing `if (isTauriApp()) { invoke(...) } else { fetch(...) }` in a service module, that is an architectural violation. `services/platform/` is where that distinction is absorbed: `detect.ts` is the one module that reads `isTauriApp`, exposing it as `isDesktop()`, and its sibling modules that reach for OS APIs carry a `TRANSPORT_EXCEPTION:` comment saying why.

---

## UI Conventions

The design system already exists — use it rather than reinventing it inline:

- **Icons: `lucide-react` only, via `<Icon icon={...} />`** (`src/components/ui/Icon.tsx`). No emoji or unicode dingbats (`👈 🔽 🔍 ⚡ ✓ ✗ ▶ ▼`, etc.) anywhere in JSX or string literals — they render as full-colour, double-width glyphs that clash with lucide's thin monochrome strokes and can't inherit `currentColor`. This is enforced by an ESLint `no-restricted-syntax` rule (see `eslint.config.js`); it is not a style preference you can opt out of.
- **Buttons: the `Button` primitive** (`src/components/ui/Button.tsx`), not raw `<button>`. It encodes a 4-level hierarchy — `primary` (one CTA per surface) → `secondary` (default action) → `outline` (emphasis without fill) → `ghost` (minimal) — plus semantic variants (`danger`, `success`, `warning`) and a `link` variant for inline text actions. Not yet lint-enforced (there is a large pre-existing surface of raw `<button>`s); new and touched code should still prefer it.
- **Colour is semantic, never decorative.** `primary` = action, `success` = running/healthy, `warning` = degraded, `danger` = destructive/failure. A fact about a model (its quantization, its parameter count, its throughput) is not a state and should not borrow a state colour. An idle/stopped state is not a failure — it gets `--color-offline` (GUI) or `style::MUTED` (CLI), not danger red.
- **Spacing and radius come from the token scale** (`--spacing-*`, `--radius-*` in `src/styles/base/variables.css`, bridged into Tailwind's `p-xs/sm/md/base/lg/xl`, `rounded-sm/base/md/lg/xl`), not raw Tailwind numerics (`p-2`, `rounded-[6px]`) or arbitrary bracket values, except where a value is genuinely one-off (e.g. matching an icon's exact pixel size).
- **Reach for the existing primitives** (`src/components/primitives/`: `Card`, `Row`, `Stack`, `Label`, `EmptyState`, `Skeleton`) before writing a bespoke `flex` wrapper or empty-state block by hand.
- **Files stay small and single-responsibility.** `scripts/check_file_complexity.sh` and `scripts/check_rust_complexity.sh` hold a 300-LOC budget as a *ratchet*, both in CI: a file already over it may shrink but not grow, and a file under it may not cross. `--update` records a deliberate growth as a visible line in the diff. When a component grows past that, extract by responsibility (see `ModelInspectorPanel/` or `SettingsModal/fields/` for the pattern: a thin composition root plus small, named child components and a barrel `index.ts`), not by splitting arbitrarily in half.

---

## Model Architecture Registry

The proxy uses a two-layer system to decide how to preprocess requests before they reach llama-server:

| Layer | Location | When |
|---|---|---|
| Chat template analysis | `gglib-core::domain::capabilities::infer_from_chat_template` | At model import — reads `tokenizer.chat_template` from the GGUF |
| Architecture registry | `gglib-core::domain::capabilities::capabilities_from_architecture` | At model import — reads `general.architecture` as a backstop |

The result of both layers is **OR-combined** and stored in the database as `Model.capabilities`.  The proxy reads this value once per request — there is no second inference pass at forward time.

### Template analysis — positive vs. negative system-role signals

`infer_from_chat_template` uses two priority tiers for the `SUPPORTS_SYSTEM_ROLE` flag:

| Priority | Pattern | Meaning |
|---|---|---|
| **1 — positive** | `[SYSTEM_PROMPT]` in template | Mistral v7: system role handled natively |
| **1 — positive** | `[AVAILABLE_TOOLS]` in template | Mistral v3/v3-tekken: system prepended inline |
| **2 — negative** | `"Only user, assistant and tool roles…"` | Old Mistral v1/v2: system role rejected |
| **2 — negative** | `"got system"` / `"Raise exception for unsupported roles"` | Other strict models |
| **default** | No signal | System role assumed supported |

Positive evidence wins: if a template contains `[SYSTEM_PROMPT]` **and** a generic error-raise, the positive signal takes precedence and `SUPPORTS_SYSTEM_ROLE` is set.

### Known architecture registry

| `general.architecture` | Models | Flags |
|---|---|---|
| `"mistral"` | Mistral v1/v2 | `REQUIRES_STRICT_TURNS` |
| `"mistral3"` | Devstral, Ministral, Mistral Small 3 | `REQUIRES_STRICT_TURNS \| SUPPORTS_SYSTEM_ROLE` |

### Adding a new architecture (request side)

1. **Add an arm** to `capabilities_from_architecture()` in `crates/gglib-core/src/domain/capabilities.rs`:

   ```rust
   "myarch" => ModelCapabilities::REQUIRES_STRICT_TURNS,
   ```

2. **Add a unit test** in the same file:

   ```rust
   #[test]
   fn myarch_requires_strict_turns() {
       let caps = capabilities_from_architecture(Some("myarch"));
       assert!(caps.contains(ModelCapabilities::REQUIRES_STRICT_TURNS));
   }
   ```

3. **Fix any already-imported models** by overriding their flags directly:

   ```bash
   gglib model capabilities <id> --set requires-strict-turns
   ```

### Adding a new architecture (response side)

**Usually: nothing to add.** Response dialects are described by a
`DialectSpec` (`crates/gglib-core/src/domain/dialect.rs`) — envelope
markers plus body codecs — which detection derives automatically by
executing the model's own chat template
(`crates/gglib-gguf/src/capabilities/template_probe.rs`) and persists on
the model row. Any family whose template renders tool calls as
`MARKERS{json}MARKERS` works with zero code: the spec drives the
`DelimitedToolCallParser` and the decode-time GBNF grammar alike.

Code is only needed for a genuinely new **body codec** — a dialect whose
tool-call body is not `{"name", "arguments"}` JSON and not the
`<function=...>` inner XML:

1. Add a variant to `BodyCodec` in `crates/gglib-core/src/domain/dialect.rs`.
2. Implement its decoder in `crates/gglib-core/src/normalize/parsers/delimited.rs`
   (`finalize_tool_call` dispatches on the spec's codec list).
3. Teach the probe's payload validation to recognize the codec's rendering
   (`template_probe::find_payload`), or — for a builtin-style dialect — add a
   `format:*` constant to `crates/gglib-core/src/normalize/tags.rs` and map
   it to a spec in `dialect_for_tags()`
   (`crates/gglib-core/src/normalize/registry.rs`).

The `normalize` pipeline (shared by the proxy and the in-process agent
path) picks up specs automatically via `get_parser()`.

### Capability overrides

Users can view and override capability flags at any time via:

- **CLI**: `gglib model capabilities <id> [--set FLAG] [--unset FLAG]`
- **API**: `PATCH /api/models/{id}/capabilities` with JSON body `{ "requiresStrictTurns": true }`

Both surfaces call the same `ModelOps::set_capabilities()` method in `gglib-app-services` — no business logic lives in the surface crates.

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

**Surface crates** (`gglib-cli`, `gglib-axum`, `gglib-tauri`) — May depend on anything in lower layers. Must not depend on each other, with one documented exception: `gglib-cli` may depend on `gglib-axum` to start the Axum HTTP server for the `gglib web` command — splitting that single call site into a fourth surface crate would be more churn than the exception is worth. `scripts/check_boundaries.sh` allow-lists exactly this edge; any other surface-to-surface dependency is a violation.

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

A fourth surface — [Architecture Decision Records](#architecture-decision-records) — records *why* a decision was made, which none of the three below are for.

### Surface 1: Crate READMEs (shields.io badges + ASCII architecture diagrams)

Each crate's `README.md` serves two narrow purposes:

1. **Badges** — metrics surfaced as shields.io endpoint badges (tests, coverage, LOC, complexity) that read from the `badges` branch (see [Badges Pipeline](#badges-pipeline) below).
2. **Architecture ASCII diagrams** — a text diagram showing where the crate sits in the layer model and an internal structure diagram. These are written and maintained by hand.

Crate READMEs are **not** the place for API documentation, usage examples, or explanatory prose about how individual types work — that belongs in Rustdoc.

### Surface 2: Module-level documentation (`README.md` + `include_str!`)

Every public module has a `README.md` alongside its `mod.rs`. The README is the canonical location for module-level documentation and is pulled into `cargo doc` via an inner attribute at the top of `mod.rs`:

```rust
#![doc = include_str!("README.md")]
```

**Do not write `//!` blocks.** The README is the single source of truth — rustdoc, GitHub, and the CI README-coverage check all read from it. Writing `//!` in addition to a README causes duplicate content in the generated docs.

#### README structure

Each submodule README must contain these two marker pairs (used by CI and badge generation):

```markdown
<!-- module-docs:start -->

What the module is responsible for.
What it is **not** responsible for (prevents scope creep).
If part of a streaming pipeline, a table of consumers.

<!-- module-docs:end -->
```

```markdown
<!-- module-table:start -->
| Module | Tests | Coverage | LOC | Complexity |
|--------|-------|----------|-----|------------|
<!-- module-table:end -->
```

**Example** (`crates/gglib-runtime/src/llama/download/README.md`):

```markdown
# download

![LOC](https://img.shields.io/endpoint?url=...)
![Complexity](https://img.shields.io/endpoint?url=...)

<!-- module-docs:start -->

Pre-built llama.cpp binary download support.

`download_prebuilt_binaries` emits [`LlamaProgressEvent`] on a
`tokio::sync::mpsc::Sender` and is consumed by:

| Consumer | Output                                                 |
|----------|--------------------------------------------------------|
| CLI      | `indicatif` progress bar                               |
| Axum     | SSE stream at `POST /api/config/system/install-llama`  |
| Tauri    | `llama-install-progress` event to the WebView          |

It is **not** responsible for rendering: no `println!`, no progress bar, no
knowledge of a terminal, an HTTP response or a WebView.

<!-- module-docs:end -->

<!-- module-table:start -->
<!-- module-table:end -->
```

#### Cargo doc link syntax in READMEs

Because the README is included via `include_str!`, it is processed as rustdoc. You can use cargo doc link syntax inside the `module-docs` section:

```markdown
[`DownloadEvent`], [`build_llama_cpp`], [`crate::domain::Model`]
```

These links resolve at `cargo doc` time and show as hyperlinks in the generated docs. They appear as plain text on GitHub — that is acceptable.

#### Adding a new Rust module

1. Create `README.md` next to `mod.rs` with the structure above.
2. Add `#![doc = include_str!("README.md")]` as the **first line** of `mod.rs`.
3. Fill the `<!-- module-docs:start/end -->` section with a description, ownership boundaries, and any consumer tables.
4. Run `bash scripts/check_readmes.sh --strict` locally — CI enforces this and will fail if the README is missing, incomplete (contains `TODO:`), or if `mod.rs` is missing the `include_str!` attribute.

#### Clippy and README content

Because the README is compiled as rustdoc, Clippy's `doc_markdown` lint applies to it. Identifiers that look like Rust code must be wrapped in backticks:

- `HuggingFace` → `` `HuggingFace` ``
- `Q4_0`, `Q8_0` → `` `Q4_0` ``, `` `Q8_0` ``
- `SQLite` → `` `SQLite` ``
- `ReAct` → `` `ReAct` ``

Snake_case module names used as top-level headings must be written in title case (`# Context Pruning`, not `# context_pruning`) to avoid the same lint.

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

## Architecture Decision Records

`docs/adr/NNNN-kebab-title.md`, numbered sequentially, never renumbered.

An ADR records a decision and the evidence behind it. Rustdoc says what the code does; an ADR says why it is that way and what would have to change for it to be different. The two are complements — a module implementing a decision should link its ADR, and the ADR should name the modules it governs.

### When to write one

- A behaviour is added **because llama.cpp cannot do it, or does it wrong.** Classify it (see [ADR 0001](docs/adr/0001-runtime-capability-tiers.md)) and give it a deletion criterion, or it will be carried forever by default.
- A capability is **deferred to upstream.** ADR 0001's rule is that capability presence is not permission to defer; the measurement that licensed the deferral belongs in an ADR, scoped to the models and build it was taken against.
- A decision was **reached by measurement** and someone will otherwise re-derive it. [ADR 0002](docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md) exists so nobody re-runs a 60-request sweep to rediscover that upstream already enforces tool schemas.

### Conventions

- **Status, date, and dependencies in a header block.** Amend in place with a dated note rather than opening a near-duplicate.
- **Record retractions, do not delete them.** When a later finding overturns an earlier one, strike the original and say why it was wrong. ADR 0002's finding 4 overturned its own finding 1; leaving both is what stops the same reasoning error recurring.
- **State the scope of the evidence.** "60/60 on one model, one build, one schema" is a finding. "Upstream enforces schemas" is a claim the evidence does not support, and ADR 0002 was overturned by a second model precisely because that distinction was written down.
- **Cite the reproducer.** A measurement that cannot be re-run is an opinion with a table.

## Badges Pipeline

Badges in crate READMEs are **not** static images. They are shields.io endpoint badges that read JSON files from a dedicated `badges` branch. Do not author or edit badge JSON files manually.

### How the pipeline works

```
CI run (ci.yml)
  └─ cargo test --no-run --message-format=json   (builds, and names each binary's crate)
  └─ cargo test --no-fail-fast | tee rust-test-output.txt
  └─ scripts/split_test_output.py                (divides that output per crate)
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

### Where the per-crate test numbers come from

`badges.yml` counts tests out of one `rust-test-<crate>.txt` per crate. Those files used to
come from running `cargo test -p <crate>` fifteen times after the aggregate run — 23m15s of a
57m job, and wrong besides: naming a crate with `-p` changes feature unification, so
`cargo test -p gglib-runtime` ran 329 tests where the workspace build runs 352.

They now come from `scripts/split_test_output.py`, which divides the single workspace run's
output by the package each test binary belongs to. Two consequences worth knowing:

* **The crate list is no longer hand-kept.** Every package with test targets gets a file. If
  you add a crate, its badge works as soon as you add its name to `ALL_CRATES` in
  `badges.yml` — nothing needs adding to `ci.yml`.
* **Do not add `--all-targets` to the test run.** It would silently drop the doctests, which
  that run is now the only thing executing. `split_test_output.py` fails a green run that
  produced no `Doc-tests` sections, so the mistake is caught rather than absorbed.

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

- Rust 1.97.1 (managed via `rust-toolchain.toml` — `rustup` will install it automatically)
- Node.js 22.12+ (see below)
- Platform system libraries (see `scripts/check-deps.sh` for a live dependency check)

Run `make setup` for a one-command first-time setup on macOS. On Linux, review `scripts/check-deps.sh` first to install system packages.

### Node Version Management

The repo pins Node 24 via `.nvmrc` — the single source, and what CI reads. Any of the following version managers will auto-activate the right version when you `cd` into the repo:

**Recommended — [mise](https://mise.jdx.dev)** (polyglot: manages Node, Ruby, Python, and more in one tool; works on Linux and macOS)
```sh
# Install mise (once per machine)
curl https://mise.run | sh
# Add to your shell — e.g. for fish:
echo 'mise activate fish | source' >> ~/.config/fish/config.fish

# In the repo — install the pinned Node version
mise install
```

**Alternative — nvm or fnm** (Node-only managers; both read `.nvmrc` automatically)
```sh
# nvm
nvm install   # reads .nvmrc
nvm use

# fnm
fnm install   # reads .nvmrc
fnm use
```

**Manual fallback** — install Node ≥22.12 directly from [nodejs.org](https://nodejs.org) and verify with `node --version`.

> **Note:** if you ever see `EACCES: permission denied` on `npm install -g`, that means the active Node is a system-owned binary. The fix is always to switch to a version-manager-managed Node — never `sudo npm install -g`.

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

# Run all pre-commit checks in sequence: fmt, lint, check, test, lint-web,
# typecheck-web, test-web, boundaries, enforce
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
| **Architecture** | `./scripts/check-tauri-commands.sh`, `check-frontend-ipc.sh`, `check_transport_branching.sh`, `check_param_source_exhaustive.sh`, `check_settings_surfaces.sh`, `check_rust_complexity.sh`, `check_file_complexity.sh` | Tauri policy; no IPC in product routes; no frontend transport branching; no catch-all over `ParamSource`; every setting reachable; the two file-size ratchets |
| **Clippy** | `cargo clippy --all-targets --all-features -- -D warnings` | No warnings, ever |
| **Rust tests** | `cargo test` (aggregate + per-crate) | Correctness |
| **Doc tests** | `cargo test --doc --verbose` | Doc examples compile and run |
| **Frontend tests** | `npm run test:run` | TypeScript correctness |
| **Lint & typecheck** | `npm run lint -- --max-warnings 0`, `npm run typecheck`, `./scripts/check_workflow_yaml.sh` | Design-system guardrails; TS correctness; parseable workflows |
| **Cross-OS check** | `cargo test -p gglib-cli --no-run` on Linux/macOS/Windows | No platform-specific breakage |

After each successful CI run, `badges.yml` downloads the test/boundary/coverage artifacts and pushes updated badge JSON files to the `badges` branch. Shields.io badges in crate READMEs resolve from there.

Coverage is measured on every push to `main` with `cargo-llvm-cov` and feeds into the same badge pipeline.

Docs are deployed to GitHub Pages automatically when a release is published, via `docs.yml`.

---

## Issue & PR Labeling

Every issue and PR needs at least one label from each of four categories: `component:`, `priority:`, `size:`, and `type:`. Most of this is automated — the manual part is smaller than it looks.

### On issues

Opening an issue through GitHub's "New Issue" form (`.github/ISSUE_TEMPLATE/issue.yml`) requires `component:`, `priority:`, `size:`, and `type:` before you can even submit — the fields are required dropdowns, and `component:`/`type:` allow selecting more than one (issues commonly span several crates, or are simultaneously e.g. a bug and a refactor).

If an issue is created outside the form — a scripted `gh issue create`, or the raw API — the form's required fields can't apply, since GitHub only validates them in the web UI and `gh issue create`'s interactive prompts. `issue-labels.yml` is the actual backstop: it checks every issue for all four categories and applies `status: needs-triage` if any are missing, removing it automatically once the labels are added by hand. `is:issue is:open label:"status: needs-triage"` is the live worklist of anything that slipped through.

**Epics:** check the optional "Epic" box on the form (applies `type: epic`) only for an issue that itself tracks multi-phase work. Don't invent a new label per phase — use GitHub's native sub-issues (the "Create sub-issue" button on the epic) to link and order the phases. It gives the epic a live progress checklist for free and has no ceiling on phase count, unlike a per-letter label scheme.

### On PRs

`label-check.yml`'s `enrich` job does three things automatically, before the required-label check even runs:

- **Inherits `component:`/`priority:`/`type:`** from any issue the PR closes (`Closes #N`, `Fixes #N`, `Resolves #N`, or a full issue URL) — skipping any category the PR already carries, so a deliberate override always wins.
- **Auto-applies `component:`** from changed file paths, but only when the touch maps to exactly one component. Backtesting showed this is reliable when unambiguous (~68% match) and wrong most of the time when several components are touched (~63% over-labelled) — so an ambiguous touch gets a comment listing the candidates instead of a guess.
- **Never auto-applies `size:`.** Backtested against 318 merged PRs: line count predicts this repo's `size:` labels only ~49–55% of the time, because `size:` is an hours/days estimate and effort doesn't track diff volume (a 3000-line lockfile-churn PR can be `xs`; a 90-line dead-CSS removal can be `l`, because the work was figuring out what was safe to delete). Instead, the PR gets a comment with the measured diff size and the historical label distribution for PRs of that size, and a human picks.

In practice: if your PR closes a fully-labelled issue, you'll rarely touch labels on the PR at all — everything but `size:` (or all four, if the issue itself had one) inherits automatically. `Label Check` is the required status check that gates merge; its failure message says exactly which category is missing and why it wasn't auto-filled.

`scripts/backtest_label_heuristics.sh` reproduces the accuracy numbers above against current PR history, if the taxonomy or crate layout changes enough to warrant re-checking them.

---

## Pull Request Checklist

Before requesting review, confirm each item:

- [ ] `make pre-commit` passes locally — everything CI requires, including the frontend gates and the architecture checks.
- [ ] `cargo test --doc` passes.
- [ ] Any new public type or enum has `///` doc comments on all items.
- [ ] Any architectural change is documented in `//!` module-level Rustdoc. ASCII architecture diagrams belong in crate READMEs; prose API documentation does not.
- [ ] If a new module was added, `./scripts/generate_module_tables.sh` has been run and the updated badge table is committed.
- [ ] Subprocess I/O is captured with `Stdio::piped()` and read on an OS thread, not a Tokio task.
- [ ] Environment variable merging uses read-then-append, not a bare `.env()` that overwrites.
- [ ] Any feature gated behind `#[cfg(feature = "...")]` is declared correctly in all consuming `Cargo.toml` files.
- [ ] If the change adds a new long-running operation: for Tier 1 (runtime behaviour), all three surfaces (CLI, Axum, Tauri) are wired up in this PR; for Tier 2 (management/inspection), the CLI is wired and the surface gap is tracked in a linked issue.
- [ ] `Cargo.lock` is up to date and committed.
- [ ] No new dependency has been introduced from a higher layer to a lower layer.
