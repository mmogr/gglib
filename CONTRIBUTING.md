# Contributing to gglib

This document is the definitive engineering guide to how the project's own changes are made and checked. For what gglib accepts from others, see [Outside contributions](#outside-contributions).

---

## Table of Contents

1. [Outside contributions](#outside-contributions)
2. [Core Philosophy](#core-philosophy)
3. [Architecture Overview](#architecture-overview)
4. [GUI Parity Principle](#gui-parity-principle)
5. [UI Conventions](#ui-conventions)
6. [Model Architecture Registry](#model-architecture-registry)
7. [Concurrency Model](#concurrency-model)
8. [Subprocess Invocation](#subprocess-invocation)
9. [Crate Boundaries](#crate-boundaries)
10. [Documentation Standards](#documentation-standards)
11. [Architecture Decision Records](#architecture-decision-records)
12. [Badges Pipeline](#badges-pipeline)
13. [Development Workflow](#development-workflow)
14. [CI Pipeline](#ci-pipeline)
15. [Issue & PR Labeling](#issue--pr-labeling)
16. [Pull Request Checklist](#pull-request-checklist)

---

## Outside contributions

Code from outside contributors is not accepted yet. Issues, bug reports and ideas are welcome through the [issue tracker](https://github.com/mmogr/gglib/issues). The reason: the README's [License](README.md#license) section says a separate commercial license may be offered, and there is no contributor agreement that would let outside code be covered by one.

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

CI's `boundaries` job runs `scripts/check_boundaries.sh` on every pull request into `main` and every push to it, and a violation fails the build. The script reads each checked crate's direct dependencies with `cargo tree --depth 1 --all-features --target all`: normal, build and dev dependencies, optional ones, and those declared for any platform. Of the rules below, only what a bullet says `check_boundaries.sh` rejects or allow-lists is checked, and only on those direct edges; the rest, such as `gglib-db` depending on no other `gglib-*` crate, are not.

**`gglib-core`** — Domain types, port traits, error types and path utilities. May read and write local files and build the process `Command`s other crates spawn (its tokio carries the `fs` and `process` features). Must not depend on a database, HTTP, CLI or UI crate: `check_boundaries.sh` rejects `axum`, `tower`, `tower-http`, `hyper`, `reqwest`, `clap`, `tauri` and `sqlx` there.

**`gglib-db`** — May depend on `gglib-core`, `sqlx` and utility crates, and on no other `gglib-*` crate. `check_boundaries.sh` rejects `axum`, `tower`, `tower-http`, `hyper`, `clap` and `tauri` there.

**`gglib-runtime`**, **`gglib-proxy`**, **`gglib-agent`**, **`gglib-download`**, **`gglib-hf`**, **`gglib-mcp`**, **`gglib-gguf`**, **`gglib-sse`**, **`gglib-bootstrap`**, **`gglib-build-info`**, **`gglib-integration-tests`** — the infrastructure crates. May depend on `gglib-core`, `gglib-db` and each other, except `gglib-sse`, which depends on no `gglib-*` crate. Must not depend on a surface crate, and `check_boundaries.sh` rejects a direct dependency on one from every crate in this list. It also rejects `axum`, `tower`, `tower-http`, `hyper`, `clap`, `tauri` and `sqlx` in `gglib-runtime`, `gglib-agent`, `gglib-download`, `gglib-hf`, `gglib-mcp` and `gglib-gguf`, and `clap`, `tauri`, `sqlx` and `tower-http` in `gglib-sse`; `gglib-proxy` and `gglib-sse` depend on `axum`.

**`gglib-app-services`** — The backend bridge: its consumers are `gglib-axum`, `gglib-cli` and `src-tauri`. No surface-specific code. `check_boundaries.sh` rejects `axum`, `tower`, `tower-http`, `hyper`, `clap`, `tauri` and `sqlx` there.

**Surface crates** (`gglib-cli`, `gglib-axum`, `gglib-tauri`) — May depend on anything in lower layers. Must not depend on each other, with one documented exception: `gglib-cli` may depend on `gglib-axum` to host the daemon in `gglib daemon run` (which `gglib web --share-lan` also runs) — splitting that one module into a fourth surface crate would be more churn than the exception is worth. `scripts/check_boundaries.sh` allow-lists exactly this edge; any other surface-to-surface dependency is a violation.

**`src-tauri`** (package `gglib-app`) — The desktop app binary, and the one crate that depends on two surfaces: `gglib-tauri` for its Tauri event emission, and `gglib-axum` to host the daemon in-process when it cannot launch an external one. It also depends on `gglib-app-services`, `gglib-runtime`, `gglib-proxy`, `gglib-core` and `gglib-build-info`. No crate depends on it, and `check_boundaries.sh` does not check it.

If your change requires adding a dependency from a lower layer to a higher layer, reconsider the design. The dependency should flow in the opposite direction via the channel/event pattern described above.

### Feature flags in `gglib-runtime`

`gglib-runtime` uses feature flags to gate compilation of heavy subsystems:

| Feature | Includes | Use in |
|---|---|---|
| *(default)* | Inference and server management | No dependent crate: each turns on `prebuilt` or `cli` |
| `prebuilt` | Pre-built binary download support | `gglib-app-services` |
| `cli` | Source build pipeline (`build/`, `install/`) — implies `prebuilt` | `gglib-cli`, `gglib-axum`, `src-tauri` |

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

### Surface 2: Module-level documentation (a directory's `README.md`, or a file's `//!`)

Every directory below a crate's `src/`, and below `src-tauri/src/`, has a `README.md` (`check_readmes.sh` checks this). For a directory module that README is its module documentation, pulled into `cargo doc` by an inner attribute on the first line of its `mod.rs`:

```rust
#![doc = include_str!("README.md")]
```

**A directory module is documented by its README alone; a single-file module documents itself with `//!`.** A `mod.rs` that also carries `//!` lines renders them after the README in the generated docs, so what they say belongs in the README's `module-docs` section instead. A single-file module (`foo.rs`) has no README of its own, and its `//!` block is its documentation.

#### README structure

Each submodule README must contain this marker pair (checked by `scripts/check_readmes.sh`):

```markdown
<!-- module-docs:start -->

What the module is responsible for.
What it is **not** responsible for (prevents scope creep).
If part of a streaming pipeline, a table of consumers.

<!-- module-docs:end -->
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
```

#### Cargo doc link syntax in READMEs

Because the README is included via `include_str!`, it is processed as rustdoc. You can use cargo doc link syntax inside the `module-docs` section:

```markdown
[`DownloadEvent`], [`build_llama_cpp`], [`crate::domain::Model`]
```

These links resolve at `cargo doc` time and show as hyperlinks in the generated docs. They appear as plain text on GitHub — that is acceptable.

#### Adding a new directory module

A new single-file module needs only its `//!` block. A new directory module:

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

### Comments say what is true now

A comment (`//`, `///` or `//!`) states what the code keeps true now: the invariant first, then, if a reader will need the evidence, one link to it.

- **No history prose.** No "used to", "previously", "was renamed" or "since #N", and no account of how the code got here. That belongs in the commit message and the pull request.
- **Evidence is linked, not kept.** A measurement, a reproduction, a table of runs or a pull request's argument lives in its issue, its pull request, an ADR or an ADR's log. The comment keeps the invariant that evidence supports and one link to it, even when several pull requests contributed.
- **What stays:** the invariant, in the present tense; a link the invariant depends on; the `///` line every `pub` item needs (Surface 3); a safety argument, in full; a compatibility fact about a stored or wire field, with its link; every fenced block in a doc comment; and every intra-doc link definition (`[Foo]: path`) that something still uses.

Guidance, not a rule, and nothing checks it: a doc block on a non-public item is usually under 10 lines. A block that states an invariant or a safety argument is exempt however long it is.

### Doctests run in the `test` job

CI's `test` job runs `cargo test --no-fail-fast`, which runs the doctests along with the unit and integration tests; there is no separate `cargo test --doc` step. In a library crate, an untagged or `rust` code block in a doc comment, or in a README that rustdoc includes, is a doctest, so a dangling import or wrong type in one fails CI. `cargo test --doc` runs them alone; `cargo test --all-targets` does not run them at all, which is why the `test` job never passes it (see [Where the per-crate test numbers come from](#where-the-per-crate-test-numbers-come-from)). When you add a triple-backtick Rust example, make sure it compiles. If an example requires external infrastructure, mark it `no_run`:

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

`docs/adr/NNNN-kebab-title.md`, numbered sequentially, never renumbered. An ADR's log, when it has one, is `docs/adr/log-NNNN.md`.

An ADR records a decision and the evidence behind it. Rustdoc says what the code does; an ADR says why it is that way and what would have to change for it to be different. The two are complements — a module implementing a decision should link its ADR, and the ADR should name the modules it governs.

### When to write one

- A behaviour is added **because llama.cpp cannot do it, or does it wrong.** Classify it (see [ADR 0001](docs/adr/0001-runtime-capability-tiers.md)) and give it a deletion criterion, or it will be carried forever by default.
- A capability is **deferred to upstream.** ADR 0001's rule is that capability presence is not permission to defer; the measurement that licensed the deferral belongs in an ADR, scoped to the models and build it was taken against.
- A decision was **reached by measurement** and someone will otherwise re-derive it. [ADR 0002](docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md) exists so nobody re-runs a 60-request sweep to rediscover that upstream already enforces tool schemas.

### Conventions

- **A header block**: `Status`, `Date`, `Depends on`, `Supersedes` and `Superseded by`, and a `Log` line once the ADR has a log.
- **State the scope of the evidence.** "60/60 on one model, one build, one schema" is a finding. "Upstream enforces schemas" is a claim the evidence does not support, and ADR 0002 was overturned by a second model precisely because that distinction was written down.
- **Cite the reproducer.** A measurement that cannot be re-run is an opinion with a table.
- **A kill criterion must name a reading that exists.** Not a counter somebody intends to add, not a number a `debug!` emits into a log nothing collects — a reading a person can actually take, named where it is taken from. A criterion nobody can read is not a criterion; it is a promise that the decision will be revisited, and it will not be. [ADR 0011](docs/adr/0011-stagnation-is-about-prose.md)'s first criterion named `loop_guard_trips` for a question that counter cannot answer, and it took the first live reading, months later, to notice. Where the reading is a survey rather than a tally, name the command that produces it, as [ADR 0009](docs/adr/0009-fit-the-context-to-the-machine.md)'s first criterion names `gglib model explain`.
- **Record zeros with their denominators.** "0 events across 10 requests, 2026-08-28" is a reading; "none" is not, because it cannot distinguish a mechanism that does not fire from one nobody exercised. This matters most for the criteria that are *satisfied* by zeros — "if it stays at zero, delete it" — where the ambiguity is what turns a small sample into a wrong deletion.

### An accepted ADR is frozen

An ADR is frozen when it is accepted. After that its header's `Status`, `Superseded by` and `Log` lines are kept current, and otherwise it changes only by a retraction's strike, an erratum, or the one move of the readings and bookkeeping already in its body to its log. Everything else goes elsewhere:

- **A reversal is a new ADR.** Keep it short, and let its header say exactly what it replaces: `Supersedes: the <part> ADR NNNN chose, and nothing else in it`. The old ADR's `Status` and `Superseded by` lines then name the new one.
- **Readings and bookkeeping go to the ADR's log**, `docs/adr/log-NNNN.md`: a measurement taken later, a follow-up that landed, a count brought up to date. A log is appended to and never edited, and an entry that cites a line number names the commit it read that line at.
- **A retraction is a log entry plus a one-line strike.** The entry says what was wrong and why. In the ADR the retracted text is struck through, not deleted, and followed by one line that links the entry. ADR 0002 keeps finding 1 beside the finding 4 that overturned it; leaving both is what stops the same reasoning error recurring.
- **An erratum is corrected in place**: a typo, a broken link, a name that was wrong when it was written. A correction that changes what the ADR decided or found is a retraction or a reversal instead.
- **An open arc keeps a log and writes its ADR at close.** While the work a decision belongs to is still moving, its readings and interim decisions go into `log-NNNN.md`, under the number its ADR will take, and the ADR is written from the log when the arc closes.
- **Readings already in an accepted ADR move to its log once.** Readings and bookkeeping still in an accepted ADR's body move to its log verbatim, and each moved block leaves its heading behind as a stub that links the log entry. The same change may rewrite the ADR's `Date` line to list only the changes made in place to its decisions, and the log's preamble to state these rules.

### Handoff briefs

A brief that carries work between sessions is not an ADR and does not need one's ceremony. It is still read as authority by whoever picks the work up, and it is copied forward.

- **Cite a source for every number**: a file path, a PR number, or the command that produced it. A number with no source is a claim wearing a finding's clothes. Because briefs are copied forward, an uncited figure is re-cited by the next brief and arrives three documents later as established fact — the "15 defects" that circulated around ADRs 0009–0011 was 12 verified and 3 deferred, and no document anywhere held either number.
- **Say where a number cannot be re-derived** — a live counter that resets, a one-off session, a reading taken on hardware nobody else has. That is "cite the reproducer" above, applied to a document that is not an ADR: the honest move is to name the gap in the brief, not to leave the figure looking as solid as the ones beside it.

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

### Adding a badge to a new crate README

Badge URLs follow the pattern `gglib-{crate-name}-{metric}.json` on the `badges` branch. For a new crate `gglib-foo`:

```markdown
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-foo-complexity.json)
```

The badge JSON files will appear on the `badges` branch automatically after the first CI run that includes the new crate. Until then, the badges render as "unknown" — that is expected.

To scaffold READMEs for new directories, use `scripts/generate_submodule_readmes.sh --create`. It writes a stub wherever a README is missing in a directory below a crate's `src/`, below `src-tauri/src/` or below the TypeScript `src/`, and in `tests/` or a directory below it. A stub under a `src/` has a title, LOC and complexity badges, and the `module-docs` markers around the `//!` text of the directory's `mod.rs`, or around a `TODO:` line when there is none; a stub under `tests/` has a title and a `TODO:` line. For each directory it stubs whose `mod.rs` lacks `#![doc = include_str!("README.md")]`, it adds that line and puts a `// MIGRATION` comment above any `//!` block. It never deletes a `//!` block: delete it, and the comment, yourself, since a module with a README carries no `//!` block (see Surface 2). It never touches a README that already exists.

```bash
./scripts/generate_submodule_readmes.sh --create           # create missing READMEs
./scripts/generate_submodule_readmes.sh --create --dry-run # preview without writing
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
# typecheck-web, deadcode-web, test-web, boundaries, enforce, bindings-check,
# doc-check
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

`.github/workflows/ci.yml` runs on every pull request into `main` and every push to it. Every PR must pass the jobs below; they are not advisory. The `CI Success` job fails if any of them failed or was cancelled. They start in parallel, except the two cross-OS jobs, which wait for `clippy`.

| Job | Runs | What it enforces |
|---|---|---|
| `fmt` | `cargo fmt --all -- --check` | Consistent code style |
| `quality` | `./scripts/check_workflow_yaml.sh`, `npm run lint -- --max-warnings 0`, `npm run typecheck` | No duplicate key in a workflow file, plus that script's `bump-version.yml` and `badges.yml` checks; the ESLint rules, warnings included; TypeScript types |
| `boundaries` | `./scripts/check_boundaries.sh`, which also runs `check_readmes.sh --strict` | What [Crate Boundaries](#crate-boundaries) says `check_boundaries.sh` rejects or allow-lists; the `gglib-bootstrap` source guard; README coverage |
| `enforcement` | `check-tauri-commands.sh`, `check-frontend-ipc.sh`, `check_transport_branching.sh`, `check_param_source_exhaustive.sh`, `check_context_floor.sh`, `check_settings_surfaces.sh`, `check_swallowed_db_errors.sh`, `check_rust_complexity.sh`, `check_file_complexity.sh`, `check_ts_bindings.sh`, `check_readme_tables.py`, all in `scripts/` | Tauri commands only in the approved files; frontend `invoke()` only with allowlisted commands; no transport branching in frontend client modules; no catch-all over `ParamSource`; nothing outside the resolver fabricates the context floor; every setting reachable from a surface; no discarded `sqlx` result; the Rust and TypeScript/CSS file-size ratchets; the ts-rs binding annotations; TypeScript README tables that match their directories |
| `test` | `cargo metadata --locked` (the workspace and `src-tauri`), `npm run build`, `cargo test --no-fail-fast`, `scripts/split_test_output.py` | `Cargo.lock` is current; the Rust tests and doctests pass; the per-crate test output the badges read, from a run that ran doctests |
| `bindings` | `make bindings-check` | The committed TypeScript bindings are what the Rust types generate |
| `rustdoc` | `cargo doc --workspace --no-deps --document-private-items --exclude gglib-app`, with `RUSTDOCFLAGS=-D warnings` | No rustdoc warning |
| `test-frontend` | `npm run deadcode`, `npm run test:run` | No TypeScript file that nothing imports, and no unused or undeclared dependency; the frontend tests pass |
| `clippy` | `npm run build`, then `cargo clippy --all-targets --all-features -- -D warnings` | No Clippy warning on Linux |
| `cli-cross-os` | `cargo test -p gglib-cli --no-run` on Linux, macOS and Windows | `gglib-cli` and its tests compile on each. Pull requests only |
| `clippy-cross-os` | `npm run build`, then `cargo clippy --all-targets --all-features -- -D warnings` on macOS and Windows | No Clippy warning on either, including in code only that OS compiles. Pull requests only |

When a CI or Coverage run on `main` completes, `badges.yml` downloads its artifacts and pushes badge JSON to the `badges` branch, where the shields.io badges in crate READMEs read it.

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

- [ ] `make pre-commit` passes locally: the Rust, frontend, architecture, bindings and rustdoc checks. The two cross-OS jobs run only in CI.
- [ ] A new doc example compiles and passes as a doctest (`cargo test --doc` runs them alone).
- [ ] Any new public type or enum has `///` doc comments on all items.
- [ ] Any architectural change is documented with its module: in the README of a directory module, in the `//!` block of a single-file module. ASCII architecture diagrams belong in crate READMEs; prose API documentation does not.
- [ ] A new or changed comment says what is true now: the invariant, at most one link, no history ([Comments say what is true now](#comments-say-what-is-true-now)).
- [ ] An accepted ADR changes only as [An accepted ADR is frozen](#an-accepted-adr-is-frozen) allows: a reading or bookkeeping goes to its `log-NNNN.md`, a reversal to a new ADR.
- [ ] Subprocess I/O is captured with `Stdio::piped()` and read on an OS thread, not a Tokio task.
- [ ] Environment variable merging uses read-then-append, not a bare `.env()` that overwrites.
- [ ] Any feature gated behind `#[cfg(feature = "...")]` is declared correctly in all consuming `Cargo.toml` files.
- [ ] If the change adds a new long-running operation: for Tier 1 (runtime behaviour), all three surfaces (CLI, Axum, Tauri) are wired up in this PR; for Tier 2 (management/inspection), the CLI is wired and the surface gap is tracked in a linked issue.
- [ ] `Cargo.lock` is up to date and committed.
- [ ] No crate has gained a dependency on a crate in a higher layer.
