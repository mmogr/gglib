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

**Files follow concepts.** A file holds one thing, whole: a type with its behaviour, one responsibility, a module's tests. Where a piece of code lives should say what it is, so that the structure itself can be read. Split a file at a real seam, where it has taken on a second job or grown a type with a life of its own, and never to make it shorter. Functions should do one thing. [File size](#file-size) below says what the 300-line budget is for.

**DRY without ceremony.** When the same logic appears twice, extract it. When extraction requires a new abstraction, make sure that abstraction earns its existence — it should simplify the call sites, not complicate them.

**No backwards compatibility obligation.** If an existing signature, struct layout, or module boundary is in the way of a cleaner design, refactor it. Callers are in the same workspace; update them. There is no public API contract to preserve here.

**Minimum viable surface area.** Do not add configuration knobs, trait objects, or generic parameters for hypothetical future requirements. The right abstraction is the one that solves the problem at hand with the fewest moving parts.

### File size

`scripts/check_file_size.sh` counts lines, once for Rust and once for TypeScript/CSS, and both run in CI through `make enforce`. Its 300-line budget is a guide, not a limit. It is there so that a file which has taken on a second job is noticed, and it was never meant to be read literally: nothing is to be shaped to fit the number.

When the check stops on a file, the question is whether the file is still one thing.

- **It has taken on a second job.** Split it at that seam, so that each file holds one responsibility, whole. A type keeps its behaviour with it. A module's tests in `x_tests.rs` beside it are the house layout, and the tests are whole there.
- **It is one thing that grew.** Keep it whole and record the growth: raise the file's row in `scripts/rust-complexity-baseline.txt` or `scripts/ts-complexity-baseline.txt` by hand, or add one, so the diff shows the number going up, and say in the pull request why the file is one concept. Lower a row when its file shrinks.

Never add a sibling file to get under the number. Half of a type's methods, one test moved out of a full test file, a list parked beside the module that reads it: each passes the check and leaves a reader two files to open for one idea.

[`scripts/README.md`](scripts/README.md#check_file_sizesh) has the check's mechanics: the baseline's rows, and what `--update` rewrites.

---

## Architecture Overview

The workspace is organized into layers, and no crate depends on a layer above its own. [`crates/README.md`](crates/README.md#architecture-overview) has the layer diagram and the crate catalog, and [Crate Boundaries](#crate-boundaries) has the dependency rules. What that diagram calls the adapter layer, this document calls the *surfaces*: the CLI, the daemon's HTTP API and the desktop app.

**Surface crates** (`gglib-cli`, `gglib-axum`, and `src-tauri` for the desktop app) adapt the shared backend to their output medium. They contain no business logic. Any feature added to one surface must be achievable on all three; how quickly the other surfaces must follow depends on the capability's tier — see the [GUI Parity Principle](#gui-parity-principle).

---

## GUI Parity Principle

Parity is tiered. Every capability must remain *achievable* on all three surfaces (the shared backend guarantees that), but when the other surfaces ship depends on what kind of capability it is:

- **Tier 1 — runtime behaviour.** Anything that changes what the proxy or runtime *does*: request handling, model lifecycle, launch flags, admission, security posture. A Tier 1 capability must be reachable through all three surfaces, and all three ship in the same PR.
- **Tier 2 — management and inspection.** Operator conveniences for looking at or arranging things (e.g. `gglib model explain`, `gglib up`, documentation). These may land CLI-first, provided the PR says so and links a tracked issue for the surface gap — parity debt must be explicit, never silent.

State the tier in the PR description (recent PRs also carry it in the commit message). When in doubt, treat it as Tier 1.

The event-channel pattern below is what makes Tier 1 parity cheap — use it for any long-running operation regardless of tier. Downloads, builds, agent loops, and model management all follow the same pattern:

1. **Core logic in a runtime or domain crate** — emits typed events over a `tokio::sync::mpsc::Sender<T>` channel. It has no knowledge of the terminal, HTTP, or Tauri.
2. **Surface adapters consume the channel** — the CLI renders events as an `indicatif` progress bar; the Axum layer streams them as SSE, which the desktop app's WebView reads as the browser does.

Concrete examples of established patterns:

| Domain | Event type | CLI consumer | Axum consumer | Tauri consumer |
|---|---|---|---|---|
| Agent loop | `AgentEvent` | spinner + streaming print | SSE at `POST /api/agent/chat` | same SSE stream — no Tauri event |
| llama install | `LlamaProgressEvent` | spinner + progress bar via `render_install_events` | SSE at `POST /api/config/system/engines/llama/install` | same SSE stream — no Tauri event |
| image runtime install | `LlamaProgressEvent` (the same events, the download phase labelled by `InstallPhase::label_for`) | spinner + progress bar via `render_install_events` (`gglib config sd install`) | SSE at `POST /api/config/system/engines/stable_diffusion/install`, the same event names | same SSE stream — no Tauri event |
| llama build | `BuildEvent` | spinner + progress bar via `render_build_events`, for an install and an update alike, and for stable-diffusion.cpp's build | none: a source build is the command line's, and the daemon installs downloads | none |

Every row is a claim about code that exists. The `llama install` row was not
one for a long time: the event type was declared private inside `gglib-axum`,
the runtime took a `Box<dyn Fn(u64, u64)>` that could not express a phase, and
the CLI consumer named here had never been written. Three surfaces each
invented their own statuses on top of two byte counts, which is what a missing
event type costs. Check a row before you copy it as precedent.

When adding a new long-running operation:

- Define the event enum in the relevant runtime or domain crate.
- The function signature takes `tx: tokio::sync::mpsc::Sender<YourEvent>` as a parameter.
- Wire the CLI adapter in its own function. Wire the Axum handler, which the desktop app's WebView calls as the browser does: a new operation gets no Tauri command, as the paragraph below says.
- Tier 1: all three ship in the same PR. Tier 2: the CLI ships now and the remaining surfaces are tracked in a linked issue.

**Tauri commands are OS integration only.** Product features are served over HTTP (Axum). The CI enforces that `#[tauri::command]` functions live only in a small set of approved files (`util.rs`, `app_logs.rs`). A new product feature does not get a Tauri command — it gets an Axum route that the WebView calls over HTTP, just like the browser-based UI does.

**Frontend transport is unified.** The frontend client modules must not branch on the platform. If you find yourself writing `if (isDesktop()) { invoke(...) } else { fetch(...) }` in a service module, that is an architectural violation. `services/platform/` is where that distinction is absorbed: `detect.ts` is the one module that asks whether this is the desktop app, as `isDesktop()`, and its sibling modules that reach for OS APIs carry a `TRANSPORT_EXCEPTION:` comment saying why.

---

## UI Conventions

The design system already exists — use it rather than reinventing it inline. [`src/styles/README.md`](src/styles/README.md) holds its contracts in full: the tokens, the platform boundary, what ESLint enforces and the design language.

- **Icons: `lucide-react` only, via `<Icon icon={...} />`** (`src/components/ui/Icon.tsx`). No emoji or unicode dingbats (`👈 🔽 🔍 ⚡ ✓ ✗ ▶ ▼`, etc.) anywhere in JSX or string literals — they render as full-colour, double-width glyphs that clash with lucide's thin monochrome strokes and can't inherit `currentColor`. This is enforced by an ESLint `no-restricted-syntax` rule (see `eslint.config.js`); it is not a style preference you can opt out of.
- **Buttons: the `Button` primitive** (`src/components/ui/Button.tsx`), not raw `<button>`. It encodes a 4-level hierarchy — `primary` (one CTA per surface) → `secondary` (default action) → `outline` (emphasis without fill) → `ghost` (minimal) — plus `danger` and `dangerGhost` for destructive actions and a `link` variant for inline text actions. ESLint rejects a raw `<button>` in `src/components` and `src/pages` outside the primitive layer (`ui/` and `primitives/`).
- **Colour is semantic, never decorative.** `primary` = action, `success` = running/healthy, `warning` = degraded, `danger` = destructive/failure. A fact about a model (its quantization, its parameter count, its throughput) is not a state and should not borrow a state colour. An idle/stopped state is not a failure — it gets `--color-offline` (GUI) or `style::MUTED` (CLI), not danger red.
- **Spacing and radius come from the token scale** (`--spacing-*`, `--radius-*` in `src/styles/base/variables.css`, bridged into Tailwind's `p-xs/sm/md/base/lg/xl`, `rounded-sm/base/md/lg/xl`), not raw Tailwind numerics (`p-2`, `rounded-[6px]`) or arbitrary bracket values, except where a value is genuinely one-off (e.g. matching an icon's exact pixel size).
- **Reach for the existing primitives** (`src/components/primitives/`: `Row`, `Stack`, `Label`, `EmptyState`, `Skeleton`, `Readout`, `Sparkline`) before writing a bespoke `flex` wrapper or empty-state block by hand.
- **A component file holds one responsibility.** When a component takes on a second one, extract by responsibility (see `ModelInspectorPanel/` or `SettingsModal/fields/` for the pattern: a thin composition root plus small, named child components and a barrel `index.ts`), not by splitting arbitrarily in half. `scripts/check_file_size.sh` covers TypeScript and CSS as it covers Rust. Its 300-line budget is a guide, and [File size](#file-size) says what it is for and how a file that is one thing records its growth.

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

CI's `boundaries` job runs `scripts/check_boundaries.sh` on every pull request into `main` and every push to it, and a violation fails the build. The script reads each checked crate's direct dependencies with `cargo tree --depth 1 --all-features --target all`: normal, build and dev dependencies, optional ones, and those declared for any platform. It takes the surface crates from the adapter layer in [`crates/layers.toml`](crates/layers.toml), and checks every other member as a crate that may not depend on one. Of the rules below, only what a bullet says `check_boundaries.sh` rejects or allow-lists is checked, and only on those direct edges, apart from the phone crates' rule, which `scripts/check_phone_closure.sh` checks over the whole dependency closure; the rest, such as `gglib-db` depending on no other `gglib-*` crate, are not.

**`gglib-core`** — Domain types, port traits, error types and path utilities. May read and write local files and build the process `Command`s other crates spawn (its tokio carries the `fs` and `process` features). Must not depend on a database, HTTP, CLI or UI crate: `check_boundaries.sh` rejects `axum`, `tower`, `tower-http`, `hyper`, `reqwest`, `clap`, `tauri` and `sqlx` there.

**`gglib-db`** — May depend on `gglib-core`, `sqlx` and utility crates, and on no other `gglib-*` crate. `check_boundaries.sh` rejects `axum`, `tower`, `tower-http`, `hyper`, `clap` and `tauri` there.

**`gglib-runtime`**, **`gglib-proxy`**, **`gglib-engine-llama`**, **`gglib-agent`**, **`gglib-download`**, **`gglib-hf`**, **`gglib-mcp`**, **`gglib-gguf`**, **`gglib-broadcast`**, **`gglib-bootstrap`**, **`gglib-build-info`**, **`gglib-integration-tests`**, **`gglib-test-http`** — the infrastructure crates. May depend on `gglib-core`, `gglib-db` and each other, except `gglib-broadcast` and `gglib-test-http`, which depend on no `gglib-*` crate; `check_boundaries.sh` rejects any direct `gglib-*` dependency in `gglib-broadcast`. Must not depend on a surface crate, which `check_boundaries.sh` rejects for every crate outside the adapter layer. It also rejects `axum`, `tower`, `tower-http`, `hyper`, `clap`, `tauri` and `sqlx` in `gglib-runtime`, `gglib-agent`, `gglib-download`, `gglib-hf`, `gglib-mcp` and `gglib-gguf`, `clap`, `tauri`, `sqlx` and `tower-http` in `gglib-broadcast`, `axum`, `tower`, `tower-http`, `hyper`, `reqwest`, `clap`, `tauri` and `sqlx` in `gglib-test-http`, `sysinfo` in `gglib-proxy`, which reads its host's name and free disk only through the `HostFacts` its caller hands it, and `tower`, `tower-http`, `hyper`, `clap`, `tauri`, `sqlx`, `sysinfo`, `gglib-runtime` and `gglib-proxy` in `gglib-engine-llama`, the in-process engine a phone runs in place of llama-server, whose client is `gglib-proxy`; `gglib-proxy`, `gglib-engine-llama` and `gglib-broadcast` depend on `axum`.

**`gglib-app-services`** — The backend bridge: its consumers are `gglib-axum`, `gglib-cli` and `src-tauri`. No surface-specific code. `check_boundaries.sh` rejects `axum`, `tower`, `tower-http`, `hyper`, `clap`, `tauri` and `sqlx` there.

**Surface crates** (`gglib-cli`, `gglib-axum`, `gglib-ffi`, and `src-tauri` as `gglib-app`: the adapter layer in `crates/layers.toml`) — May depend on anything in lower layers. Must not depend on each other, with two documented exceptions, both for hosting the daemon `gglib-axum` builds: `gglib-cli` may depend on it for `gglib daemon run` (which `gglib web --share-lan` also runs), and `gglib-app` for the daemon it runs in-process. Splitting that one module into a crate of its own would be more churn than the exceptions are worth. `scripts/check_boundaries.sh` allow-lists exactly these two edges; any other surface-to-surface dependency is a violation, and so is a direct dependency on a surface crate from any crate outside the adapter layer.

**`gglib-ffi`** — The Swift-facing surface, built into the dynamic framework an iPhone app embeds, and one of the phone crates below. `check_boundaries.sh` rejects `clap`, `tauri`, `axum`, `tower-http`, `hyper`, `sqlx`, `sysinfo`, `netdev`, `nix`, `iroh`, `modelpipe`, `gglib-runtime` and `gglib-app-services` there: it composes gglib's own crates, and takes no server, database or host probe of its own. The engine it serves is `gglib-proxy` in front of `gglib-engine-llama`. Its only `unsafe` is the `extern "C"` scaffolding `uniffi` expands, which the workspace's `unsafe_code` deny does not report because it comes from another crate's macro; it carries no `allow` for it.

**`src-tauri`** (package `gglib-app`) — The desktop app binary. It depends on one surface, `gglib-axum`, to host the daemon in-process when it cannot launch an external one, and holds its own Tauri event emission in `src/app/events.rs`. It also depends on `gglib-app-services`, `gglib-proxy`, `gglib-core` and `gglib-build-info`. No crate depends on it. `check_boundaries.sh` checks it only as a surface crate: no surface dependency but `gglib-axum`.

**The phone crates** — the crates a phone build compiles, which `PHONE_CRATES` in the `Makefile` lists; nothing else lists them. `make ios-check`, and CI's `ios-check` job, check them for `aarch64-apple-ios`. Their dependency closure for that target, every normal dependency at any depth with each crate's default features, must not reach `sysinfo`, `netdev`, `nix`, `iroh`, `modelpipe`, `gglib-runtime` or `gglib-app-services`: `scripts/check_phone_closure.sh` rejects any of them and prints the path that reaches it, and `make boundaries` runs it. It reads `cargo tree` and builds nothing, so it needs no iOS toolchain. A crate whose closure reaches one of them joins the list once that dependency is behind a port its caller fills.

If your change requires adding a dependency from a lower layer to a higher layer, reconsider the design. The dependency should flow in the opposite direction via the channel/event pattern described above.

---

## Documentation Standards

This codebase has three distinct documentation surfaces. Each has a defined purpose and a defined location. Understanding the split prevents duplication and keeps the right audience reading the right thing.

A fourth surface — [Architecture Decision Records](#architecture-decision-records) — records *why* a decision was made, which none of the three below are for.

### Surface 1: Crate READMEs (badges + an internal structure diagram)

Each crate's `README.md` serves two narrow purposes:

1. **Badges** — metrics surfaced as shields.io endpoint badges (tests, coverage, LOC, complexity) that read from the `badges` branch, in a header block `make readmes` writes (see [Badges Pipeline](#badges-pipeline) below).
2. **An internal structure diagram** — a text diagram of the crate's modules, drawn by hand; `scripts/check_diagram_names.py` checks that every name in it exists in the code. Where the crate sits is the generated graph in [`crates/README.md`](crates/README.md#architecture-overview), which the header block links.

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

**Example**, abridged from `crates/gglib-runtime/src/llama/download/README.md`:

```markdown
# download

<!-- module-docs:start -->

Pre-built llama.cpp binary download support.

`download_prebuilt_binaries` emits [`LlamaProgressEvent`] on a
`tokio::sync::mpsc::Sender` and is consumed by:

| Consumer | Output                                                        |
|----------|---------------------------------------------------------------|
| CLI      | `indicatif` progress bar                                      |
| Axum     | SSE stream at `POST /api/config/system/engines/llama/install` |

The desktop app reads the Axum stream; it has no consumer of its own.

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

`./scripts/generate_submodule_readmes.sh --create` writes a stub wherever a README is missing in a directory below a crate's `src/`, below `src-tauri/src/` or below the TypeScript `src/` (except `src/types/generated/`, which is ts-rs output that `check_readmes.sh` also skips); `--dry-run` lists them without writing. A stub has a title and the `module-docs` markers around the `//!` text of the directory's `mod.rs`, or around a `TODO:` line when there is none. In a directory it stubs whose `mod.rs` lacks the `include_str!` line, the script adds the line and puts a `// MIGRATION` comment above any `//!` block: delete that block and the comment yourself. It never touches a README that already exists.

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

`cargo doc` is deployed to GitHub Pages by the `deploy-docs` job of `.github/workflows/release.yml`, which runs for each release that is not a prerelease. It runs:

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

The version, test, coverage, LOC and complexity badges in the root README, in `crates/README.md` and in each crate's own `README.md` are shields.io endpoint badges that read JSON files from the `badges` branch. Nothing on that branch is written by hand.

[`crates/layers.toml`](crates/layers.toml) is the one list of crates. Every workspace member has a row there, and a crate has badges unless its row says `badges = false`, with a `why`. For each crate with badges the branch holds four files:

| File | Counted from | Written after |
|---|---|---|
| `<crate>-tests.json` | The CI run's `rust-test-<crate>.txt` (see below): tests passed over tests run, from every `test result:` line | CI |
| `<crate>-coverage.json` | The Coverage run's `<crate>-lcov.info`. `coverage.yml` measures every crate with badges, less any whose row says `coverage = false` | Coverage |
| `<crate>-loc.json`, `<crate>-complexity.json` | `scc` over the crate's directory, at the commit the run tested | CI |

The root README's version badge reads `version.json`, written from `Cargo.toml` by the same CI run.

`badges.yml` runs `scripts/write_badges.py` after each successful CI or Coverage run of a push to `main`; a red or cancelled run changes nothing. The script deletes every JSON file on the branch outside that set, so the branch holds no file a README does not read. A badge reads `none` when the run's artifact had no file for its crate, and `pending` until the first run that writes it. The branch's own README lists the files. No README below a crate's top level carries a badge, because the pipeline writes no file for a module.

A crate's README carries its badges, and a line naming its layer that links the workspace diagram, in a block between `<!-- crate-header:start -->` and `<!-- crate-header:end -->`, which `make readmes` writes from `crates/layers.toml`. `make enforce` runs `scripts/crate_list.py --check`, which fails when a member has no row, a crate depends on a layer above its own, a header block is out of date, or a Markdown file links a file on the `badges` branch that the pipeline does not write.

A new crate gets a row in `crates/layers.toml`, then `make readmes`. Its badges read `pending` until the next CI and Coverage runs on `main`.

The README's status card lives on the same branch, as `status-light.svg` and `status-dark.svg`. `status-card.yml` writes them each morning with `scripts/status_card/status_card.py`, which collects the releases, llama.cpp's pin and the merged pull requests through `gh` and draws both files from that. `write_badges.py` deletes only JSON files, so neither pipeline removes the other's. A pull request that changes the script runs its self-test, which compares the light card drawn from a fixture with a golden copy; the script's header says how to refresh that copy after a deliberate change to the drawing.

### Where the per-crate test numbers come from

`scripts/split_test_output.py` cuts one `rust-test-<crate>.txt` per crate out of the single workspace `cargo test` run, by the package each test binary belongs to. Two consequences worth knowing:

* **The `test` job keeps no crate list.** Every package with test targets gets a file.
* **Do not add `--all-targets` to the test run.** It would silently drop the doctests, which that run is the only thing executing. `split_test_output.py` fails a green run that produced no `Doc-tests` sections, so the mistake is caught rather than absorbed.

---

## Development Workflow

### Prerequisites

- Rust 1.97.1 (managed via `rust-toolchain.toml` — `rustup` will install it automatically)
- Node.js 22.12+ (see below)
- Platform system libraries (`gglib config check-deps` is the live dependency check; `make check-deps` runs it once gglib is built, after checking the toolchains itself)

Run `make setup` for a one-command first-time setup on macOS. On Linux, run `cargo run -p gglib-cli -- config check-deps` first: it lists the system packages to install.

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

# Run all pre-commit checks in sequence: fmt, lint, test, lint-web,
# typecheck-web, deadcode-web, test-web, boundaries, unused-deps, enforce,
# bindings-check, doc-check
make pre-commit
```

### Your data while developing

A build from this checkout uses the same data folder as an installed gglib:
your real library, settings and llama.cpp install (`gglib config paths` shows
the folder and which binary is running). `make install` warns when another
`gglib` on `PATH` is shadowed. `make uninstall` asks three things before it
deletes anything, each with No as the default: whether to remove the build,
whether to delete that folder and the desktop app, and whether to delete your
downloaded models.

To work against a library of your own, for example while changing the
database schema, give the build its own folder and its own daemon port:

```bash
export GGLIB_DATA_DIR="$PWD/.dev" GGLIB_DAEMON_PORT=9888
cargo run -p gglib-cli -- up
```

A change to the database schema raises `SCHEMA_GENERATION` in
`crates/gglib-db/src/setup_generation.rs` and rewrites the snapshot with
`GGLIB_UPDATE_SCHEMA_SNAPSHOT=1 cargo test -p gglib-db schema_snapshot`; the
`schema_snapshot` test fails until both are done. An older gglib then
refuses the upgraded library instead of writing to it.

### Working on the frontend

```bash
npm ci               # Install what package-lock.json says, without rewriting it
npm run dev          # Start Vite dev server
npm run test:run     # Run Vitest suite
npm run build        # Production build (required before integration tests)
```

### Testing with feature flags

Some crates have conditional compilation gated on feature flags. A plain `cargo test` will use default features. To test a specific feature combination:

```bash
cargo test -p gglib-db --features test-utils
cargo doc  -p gglib-db --features test-utils
```

### A fake HTTP server in a test

A hand-written stand-in server in a test (one that is not `axum::serve`) takes each connection from `gglib_test_http` (`accept`, or `incoming` and then `Incoming::request`; `blocking::` for `std::net`), never from `TcpListener::accept` or `incoming` directly; `clippy.toml` refuses those, and axum's `Listener::accept`, everywhere else. The helper hands a stream over only once the request has been read whole, because an answer written first can reach hyper before hyper has sent its request, and hyper cancels it (#1553). A test about the connection itself (was it dialled, is it held open unanswered) takes an `Incoming` and holds or drops it without reading it.

### Lockfile discipline

`Cargo.lock` is committed and must stay consistent. Before anything builds, the `test` job runs `cargo metadata --locked` for the workspace and for `src-tauri`, which fails when the lock does not already satisfy every manifest. After editing a `Cargo.toml`, run `make check`, which records the change in the lock, and commit the result. To move one third-party crate, run `cargo update -p <crate> --precise <version>`, and name in the PR every version the lock diff moves. Never run `cargo generate-lockfile`: it re-resolves every dependency against whatever the registry holds that minute, so its diff carries third-party upgrades nobody reviewed.

### Unused dependencies

CI's `boundaries` job runs `make unused-deps`, which is `cargo shear --deny-warnings`. It fails on:

- a dependency a crate declares and none of its targets names;
- a dependency under `[dependencies]` that only the crate's `tests/` name, which belongs under `[dev-dependencies]`;
- an entry in `[workspace.dependencies]` that no member takes;
- an optional dependency nothing names;
- an `ignored` entry that is not needed.

[cargo-shear](https://github.com/Boshen/cargo-shear) reads the source as text and compiles nothing. Code for another platform or behind a feature therefore counts as a use, and so does a `#[cfg(test)]` module under `src/`: a dependency that only unit tests name passes under `[dependencies]`, so put it under `[dev-dependencies]` yourself.

A dependency that no code names but the build needs goes in that crate's `ignored` list, with the reason on the line above it:

```toml
[package.metadata.cargo-shear]
# Named nowhere in code: it sets the features of the SQLite that sqlx links.
ignored = ["libsqlite3-sys"]
```

Install the tool with `cargo binstall cargo-shear` or `brew install cargo-shear`. `cargo install cargo-shear`, run from this checkout, builds it with the toolchain `rust-toolchain.toml` pins, which can be older than the tool needs.

### Dependency updates

Dependabot (`.github/dependabot.yml`) checks once a week at 06:00 (Australia/Brisbane), cargo on Monday, npm on Wednesday, GitHub Actions and the two Python requirements files on Friday, and opens grouped PRs: cargo and npm with minor and patch updates only, and GitHub Actions with majors included, except for the actions only a release runs. A new version is proposed once it is three days old. When the set of updates a group would carry changes, Dependabot opens a new PR for the group and closes the old one as superseded, so the open one is always the one to merge. Once security updates are switched on in the repository's settings, they arrive whenever an advisory does, grouped per ecosystem, labelled `security`, with `[security]` after the commit prefix in the title. A Dependabot PR is squash-merged as Dependabot wrote it: its author, subject and trailers stay as they are, and its branch is named `dependabot/…`.

- **Merge them one at a time, each brought up to date with `@dependabot rebase`.** `main` merges only a PR that is up to date with it, and between its weekly runs Dependabot does not rebase a PR that is merely behind `main`, nor any PR once it has gone 30 days unmerged. Never press **Update branch**: it pushes a merge commit onto Dependabot's branch, and Dependabot stops rebasing a PR once anyone else has pushed to it. Each merge leaves every other open PR behind, so merge the open ones in one sitting.
- **Merge nothing while `main`'s version is untagged.** `release.yml` releases any version without a `v<version>` tag from the next push to `main` whose CI passes. If the release of a merged release PR or promotion failed, re-run it first, or the next dependency merge ships that version from its own tree. The release PR follows the same rule: `release-please.yml` runs after a green CI run on `main` and fails at once if that version is not tagged.
- **A red PR is fixed in a PR of your own.** Open one that makes the same bump and the code fix together, titled for what the change does, and never push onto Dependabot's branch. For the `ts-rs` group the fix is usually `make bindings`. Once yours merges, comment `@dependabot recreate` on the group PR, which closes it, and opens a new one from `main` if the group still has updates to carry.
- **Majors and Tauri minors are bumped by hand, with their partners.** For a `0.y` crate a new `y` counts as a major. The npm `0.x` packages (`@assistant-ui/react`, `lucide-react`) get patches only, in `npm-zero-major`, because npm counts 0.11 → 0.15 as a minor. `tauri build` refuses to run when the `tauri` crate and `@tauri-apps/api`, or `tauri-plugin-dialog` and `@tauri-apps/plugin-dialog`, differ in major or minor, and no PR job runs it, so each pair moves together in one PR. A security update ignores these rules, and a patch elsewhere can drag a Tauri minor in through `Cargo.lock`: close such a PR and make the bump in your own, with its npm twin. `gtk` and `libloading` stay on the versions Tauri resolves, and `sqlx` moves with `libsqlite3-sys`, because the build can hold only one `libsqlite3-sys`.
- **Test-run what only a release exercises.** PR CI never runs `release.yml`. Before merging a `tauri-crates` or `tauri-npm` PR, or an Actions PR that changes `release.yml`, run `gh workflow run release.yml --ref <its branch> -f test_run=true` and wait for it to pass. A test run skips the steps that tag, publish, attest and deploy, so a major of an action used only there, or of `actions/github-script`, is bumped by hand.
- **`brace-expansion` is held by `overrides` in `package.json`**, not by a range Dependabot edits. A fix that needs a version outside `^5.0.8`, or outside `^1.1.17` under `minimatch@3`, is made there by hand; `//overrides` says why the carve-out stays.
- **To update without waiting for Dependabot, run Update dependencies.** Actions → Update dependencies → Run workflow, or `gh workflow run update-deps.yml -R mmogr/gglib [-f ecosystem=npm|both] [-f package=<name>]`, opens one PR with the newest versions the ranges allow, even ones under three days old, listing every package that moved, or opens nothing when nothing is newer. It takes Tauri minors too, and its PR says when Tauri moved.

### The engine pins

gglib installs one release of each engine: llama.cpp's `PINNED_LLAMA_RELEASE` (`crates/gglib-core/src/domain/engine_pins.rs`, which `scripts/llama_pin.sh` prints) and stable-diffusion.cpp's `PINNED_SD_RELEASE` (`crates/gglib-runtime/src/sd/install/release.rs`). Every install takes the pin, whether it is a download or a source build, which clones the pinned tag; an update is an install of the pin over an older one. [ADR 0001](docs/adr/0001-runtime-capability-tiers.md) makes moving a pin a deliberate event: bump the constant, record `contracts/upstream/` again, run the suite, and ship the bump in a PR of its own.

- llama.cpp tags a `b<n>` release on nearly every merge. Since about `b10566` (2026-08-21) they are marked prereleases. The `v*` releases (`releases/latest`) carry no binaries, only a `nightly-tag.txt` naming the `b<n>` they point at. Take that `b<n>` when moving the pin.
- stable-diffusion.cpp tags `master-<n>-<commit>`. Take the newest by `n`, never `releases/latest`, which can lag.
- `engine-pins.yml` proposes each bump. Every Monday, or when run by hand, it reads each engine's releases that are three days old and not prereleases, takes the candidate by the two rules above, edits the pin's one line, records the candidate into `contracts/upstream/` as CI does, and opens a `chore(deps):` pull request from `chore(deps)/<engine>-<tag>` with the pin and the recordings in one commit. Its body leads with the ADR readings that moved. It proposes a candidate once, and nothing newer for an engine while one of its pin pull requests is open, so a closed candidate is not proposed again. Push fixes and new recordings onto its branch (Update branch is fine too: the workflow never rebases it). CI checks that pull request.

- **Try a release before the constant moves.** `GGLIB_LLAMA_RELEASE=<tag> gglib config llama install --force` (or `GGLIB_SD_RELEASE` and `config sd`), then `status`; `--force` installs the tag whatever is there. Without the variable, a later release reads as newer than the pin and is kept; an earlier one reads as older, and `gglib serve`, `gglib up` and the app offer the pin in its place.
- **Check what the bump meets.** CI checks it on a pull request into `main` or in a joined stack. The `CLI Check` legs on Linux and macOS install both pinned engines and run `gglib-runtime`'s ignored tests, which hold every launch flag and fixed value gglib passes to llama-server, llama-bench and sd-server to that binary's own `--help` (CI runs their `--version` directly), and every platform row of both download tables to the pinned release's assets. A failure there names the flag, the value or the row. A launch proves the flags parse, not what a real model does.
- **Read the recordings' diff.** The same legs start the pinned llama-server on two tiny models and record what it answers into `contracts/upstream/`, report-only for now: a difference shows in the step summary of `Compare with contracts/upstream`. On a bump the workflow commits the new recordings beside the pin; when its body says the recorder did not run, or you move a pin by hand, commit them from `make record-upstream` or from the Linux leg's `upstream-recordings` artifact unpacked into `contracts/upstream/`. A pin moved without them fails `Test (Rust)`, which holds `version.txt` to the pin. Their diff is the reading: new `--help` flags (ADR 0012's survey), new keys in `shape.txt`, and new values in `observed.json`.
- **The in-process engine is recorded beside llama-server.** A phone runs llama.cpp in-process (`gglib-engine-llama`) where a desktop starts llama-server, and `contracts/upstream/llama-inproc/` is what that engine answered the recorder's questions with, on the same models, at the pin: `make record-upstream-inproc` on a Mac, after `make phone-llama` and with `GGLIB_ENGINE_MODELS_DIR` set. `Test (Rust)` holds it to `llama/` on every OS from the committed files: each `shape.txt` line one llama-server answered, the lines gglib reads present, its `version.txt` at the pin, each pinned template's render byte for byte, and tinygemma3's greedy reply the same: its visible pieces, its token count and its `finish_reason`. On a llama.cpp bump the workflow's macOS job takes the Linux job's recordings, builds `llama.xcframework` at the candidate, records the in-process engine beside them and hands both over, so the pull request carries both; when its body says the in-process engine was not recorded, record it with `make record-upstream-inproc`, or from the macOS leg's `upstream-recordings-aarch64-apple-darwin` artifact. A function the candidate renames or retypes fails `phone-llama`; a render or a reply the engine no longer gives alike fails `Test (Rust)`, which names the template or the reading.
- **Approve each ADR reading that moved.** Every reading in `observed.json` that an ADR's criterion names carries the quote, and the `ADR criterion moved` check fails until `contracts/upstream/llama/approvals.json` agrees with it. The note says what was decided: `deleting <stage> (#<issue>)`, `keeping, because <reason>`, or `probe changed in this PR` when a gglib change moved it. `make adr-readings` runs the check and lists the readings no recording takes.
- **Take a real picture before merging a stable-diffusion.cpp bump**, with ADR 0018's render-beside-a-chat pair. A llama.cpp bump needs no chat by hand, and neither the squash message nor the ADR logs carry readings: the recordings are the record. On a llama.cpp bump the workflow also puts Qwen3.5-0.8B through three probes (a tool call, split reasoning, an image in a tool message) into `contracts/upstream/llama/tier3/`, which no check reads.
- **llama.cpp for the phone is built at the pin.** Upstream's released `llama-<tag>-xcframework.zip` has no iOS simulator slice, so `make phone-llama` builds llama.cpp's own `llama.xcframework` instead: `scripts/apple/build-llama-xcframework.sh` runs upstream's `build-xcframework.sh`, unchanged, for `ios-device`, `ios-sim` and `macos` in a shallow clone of the pinned tag, into `target/apple/llama-<pin>/` with a zip and `pin.txt`, and `scripts/apple/check-llama-xcframework.sh` holds each slice to the `llama.h` functions the phone engine calls. A function a bump renames or drops fails that check, in CI's `phone-llama` job, whose cache is keyed by the pin, the runner's Xcode and the build script, so a bump builds the framework again. The same job then compiles the phone engine's C shim (`crates/gglib-engine-llama/csrc/`) against the pin's `llama.h`, where a function a bump retypes or deprecates is a compile error, runs the decoder's tests on the Mac's slice with the recorder's tiny models, and lints it for the Mac, the iPhone and its simulator; `make engine-llama-test engine-llama-clippy` does the same locally, after `make phone-llama` and with `GGLIB_ENGINE_MODELS_DIR` set. A call the shim gains goes into the check's list in the same change, which the check holds it to.

### The Swift framework

`gglib-ffi` is the crate an app links, as `GglibFFI.xcframework`: a dynamic framework for iOS, the iOS simulator and macOS (`aarch64` only), with the Swift `uniffi` generates for it committed in `crates/gglib-ffi/swift/`. Each slice links llama.cpp's own `llama.framework` at the pin (the crate's `llama` feature), which an app embeds beside it. The crate builds and tests as plain Rust on every OS, with a scripted decoder behind its served engine, so `make pre-commit` covers it on Linux. Its README has the layout; the scripts in `scripts/apple/` each say what they do.

- **A change to its exported surface regenerates the binding.** Run `make ffi-binding` and commit `crates/gglib-ffi/swift/` with the Rust. The generated Swift carries `uniffi`'s API checksums and stops with a fatal error at its first call when they disagree with the framework's, so `ffi-apple`'s `make ffi-binding-check` fails a pull request whose committed copy is stale.
- **On a Mac with Xcode, `make phone-llama ffi-xcframework-fast ffi-smoke`** builds llama.cpp's framework at the pin and this one against it into `target/apple/`, checks each slice, and calls it from a scratch Swift package, which serves tinygemma3 from `GGLIB_ENGINE_MODELS_DIR` and streams one reply from it. `make ffi-xcframework` is the optimised build a release ships. Neither boots a simulator.
- **A local Xcode is not CI's.** `ffi-apple` builds with the runner's default Xcode, which is what ggchat's CI builds the app with. A local build with another Xcode proves the scripts; CI proves the framework.
- **A release attaches the frameworks and the Swift.** `release.yml`'s `build-phone` job restores or builds `llama.xcframework` with the `llama-xcframework` action, runs `make ffi-xcframework` and `make ffi-smoke` with the runner's default Xcode, then `scripts/apple/package-release.sh`, which writes `GglibFFI-<version>.xcframework.zip` (the XCFramework at the archive's root, where SwiftPM looks for a binary target's framework), `GglibFFI-swift-<version>.zip` (the generated Swift, header and modulemap, and a `binding.json` naming the gglib version, `uniffi` version and llama.cpp pin) and `llama-<version>.xcframework.zip` (the zip `build-llama-xcframework.sh` made of llama.cpp's framework at the pin), and checks all three as an app meets them. They go into `SHA256SUMS` and carry provenance like every other asset; each framework archive's line there is what `swift package compute-checksum` reports, so it is the checksum an app pins. `checksums` waits for this job as for the others, so a failed framework build publishes no `SHA256SUMS`. Locally, `make phone-llama ffi-xcframework ffi-smoke` and then `scripts/apple/package-release.sh <version> <dir>` run the same steps.
- **`uniffi` stays on the minor modelpipe-ffi uses.** Both bindings sit in one app and their headers share the definitions behind `UNIFFI_SHARED_H`, so a move to another minor is made in both repositories together; `build_info`'s test fails on any other.

### Stacked pull requests

A change that splits into several concerns goes up as a stack: one PR per concern, one commit per PR, each PR based on the branch of the PR below it, and the bottom one on `main`. The chain is joined into a GitHub stack ([stacked pull requests](https://docs.github.com/en/pull-requests/reference/stacked-pull-requests), a public preview). In a stack, CI runs on every PR and not only on the one into `main`, a merge lands a PR together with every PR below it, and GitHub rebases the PRs that are left.

- **Join the chain into a stack when its PRs are opened.** `gh stack submit` opens the PRs and the stack together. `gh stack link <bottom> … <top>` joins PRs that already exist, by number or by branch. On the website, a new PR whose base is another PR's branch offers **Create stack**. A chain that is not joined gets none of the above: its upper PRs get no CI run, because `ci.yml` runs only for a PR into `main`, and nothing rebases them after a merge.
- **Keep the stack one line.** A stack is a single chain, so a PR has at most one PR on top of it. Two changes that build on the same PR go one after the other.
- **Fix a PR in the middle by amending its commit, then rebase the PRs above it.** From that branch, run `gh stack rebase --upstack` and then `gh stack push`. Without `gh stack`'s local tracking, rebase each branch above with `git rebase --onto <new head below> <old head below> '<branch>'`, and push them together with `git push --force-with-lease --atomic origin '<branch>' …`.
- **Merge once every PR to land is green, with squash, from the highest of them.** The merge box of a stacked PR shows the whole stack. Merging a PR lands it and every unmerged PR below it in one step, as one squash commit per PR, and the next PR up becomes the bottom. From a terminal it is `gh stack merge <PR number> --squash`. If GitHub will not merge the group, merge the bottom PR alone and wait for the next one's CI.
- **Never merge a stacked PR with the bypass.** After a merge that bypasses `main`'s ruleset, GitHub retargets the next PR and leaves it un-rebased. It did so after 21 of the 22 bypass merges counted in this account's stacks, and after 1 of the 12 merges made without the bypass.
- **Never pass `--delete-branch` to `gh pr merge` when another PR is based on that branch.** It deletes the branch itself, and GitHub then closes the PRs based on it instead of retargeting them ([cli/cli#1168](https://github.com/cli/cli/issues/1168)). The repository deletes a merged branch on its own.
- **Do not change a stacked PR's base, and do not set it to auto-merge.** GitHub refuses a base change on a PR that is in a stack, and moves the base itself when the PR below merges. Auto-merge is not supported for stacked PRs.
- **A PR that GitHub left un-rebased is rebased onto `main` past its parent's old head.** That is the state after a bypass merge, for a PR outside the stack, and when GitHub's rebase conflicts. Branch names hold parentheses, so quote them.

  ```bash
  git fetch origin
  git rebase --onto origin/main <parent's old head> '<branch>'
  git range-diff <parent's old head>..'origin/<branch>' origin/main..HEAD
  git push --force-with-lease origin '<branch>'
  ```

  `gh pr view <parent> --json headRefOid` gives the parent's old head, which GitHub keeps after the branch is gone. `range-diff` must mark every commit `=`: any other mark means the rebase changed a patch.
- **A stacked PR that gets no CI run after a push is opened again as a new PR from the same branch.** Its merge ref is gone ([github/gh-stack#319](https://github.com/github/gh-stack/issues/319)), and closing and reopening the PR does not bring it back.

### Releases

A release is cut by merging a pull request. `release.yml` is the only thing that tags: when CI passes on a push to `main` whose version has no `v<version>` tag, it tags that version and makes the GitHub release. Every stable version has first been out as a release candidate, `X.Y.Z-rc.N`, and ships exactly that candidate's code.

- **Merged work collects in the release PR.** After each CI run on `main` passes, `release-please.yml` brings a standing pull request titled `chore(main): release X.Y.Z-rc.N` up to date. release-please works out the version from the commit types since the last release, writes it into `Cargo.toml`, `package.json` and `.release-please-manifest.json`, and adds the new commits to `CHANGELOG.md`; a second commit moves the workspace's versions in both lockfiles. Never edit the release PR or `CHANGELOG.md` by hand: the next green CI run on `main` rewrites the PR from `main`.
- **Merging the release PR ships a release candidate.** `release.yml` tags `vX.Y.Z-rc.N` and makes a GitHub prerelease with its assets; apt, Homebrew and the docs are left alone. The release PR never merges itself.
- **The commit types choose the version.** Inside a cycle, each merge that adds to the changelog moves the release PR to the next `rc.N`. From a stable version, a `feat` or a `!` starts the next minor's `rc.1` (0.29.0 → 0.30.0-rc.1), and a `fix`, `perf` or `docs` without one the next patch's (0.29.1-rc.1). Every conventional type adds to the changelog: `feat`, `fix`, `perf` and `docs` under their own headings, and `refactor`, `test`, `ci`, `chore`, `build`, `style` and `revert` under "Internal". A title that is not conventional adds nothing, and so opens no candidate: write one. A candidate ships exactly what `main` holds, so nothing that changes the tree is left out of the record.
- **The commit subject is the changelog line.** release-please lists each merged commit's subject under its type's heading, and a breaking one under breaking changes too, so the PR title (the squash commit's subject) is written for someone reading the changelog.
- **`Release-As:` overrides the version.** A line `Release-As: X.Y.Z-rc.N` at the end of a merged commit's message (the squash merge box lets you add it) makes the release PR propose that version. It is needed in one case: a `feat` or a `!` merged while `main` is a patch candidate, such as 0.29.1-rc.1, makes release-please propose the stable 0.30.0, and `release-please.yml` fails rather than let a stable version through the release PR. `Release-As: 0.30.0-rc.1` puts it right.
- **Run Promote once `main` is exactly the last candidate.** Actions → Promote → Run workflow, or `gh workflow run promote.yml -R mmogr/gglib`. It turns `X.Y.Z-rc.N` into `X.Y.Z`, heads `CHANGELOG.md` with the stable version, and opens `chore(release): promote X.Y.Z-rc.N to X.Y.Z`, which merges itself once CI passes. `release.yml` then tags `vX.Y.Z` and each stable channel waits for its approval. If anything has merged since the candidate, `scripts/check_rc_promotion.sh` refuses the promotion: merge the release PR to cut another candidate, let it release, and run Promote again.

---

## CI Pipeline

`.github/workflows/ci.yml` runs on every pull request into `main` and every push to it. Every PR must pass the jobs below; they are not advisory. The macOS and Windows jobs run only when `scope` says the change can reach them, and every Linux job runs on every change. `CI Success` fails if any job failed or was cancelled, or was skipped while its `scope` output said it should run. They start in parallel, except the jobs `scope` decides, which wait for it, and the two cross-OS jobs, which also wait for `clippy`. A newer push to a pull request cancels that pull request's older run, so on a commit that is no longer the pull request's head a cancelled run, with `CI Success` failed, is expected.

| Job | Runs | What it enforces |
|---|---|---|
| `fmt` | `cargo fmt --all -- --check` | Consistent code style |
| `quality` | `npm run lint -- --max-warnings 0`, `npm run typecheck` | The ESLint rules, warnings included; TypeScript types |
| `boundaries` | `make boundaries`, which is `check_boundaries.sh` and the `check_readmes.sh --strict` it runs, then `check_phone_closure.sh` over the `Makefile`'s `PHONE_CRATES`; `make unused-deps`, which is `cargo shear --deny-warnings` | What [Crate Boundaries](#crate-boundaries) says `check_boundaries.sh` and `check_phone_closure.sh` reject or allow-list; the `gglib-bootstrap` source guard; README coverage; no dependency a crate declares and none of its targets uses (see [Unused dependencies](#unused-dependencies)) |
| `enforcement` | `make enforce` | The repository rules that neither the compiler nor a linter checks. The `enforce` recipe in the `Makefile` is the only list of them, and each script's header says what it rejects |
| `test` | `cargo metadata --locked`, `cargo test --no-fail-fast`, `scripts/split_test_output.py` | `Cargo.lock` is current; the Rust tests and doctests pass; the per-crate test output the badges read, from a run that ran doctests |
| `bindings` | `make bindings-check` | The committed TypeScript bindings are what the Rust types generate |
| `rustdoc` | `cargo doc --workspace --no-deps --document-private-items --exclude gglib-app`, with `RUSTDOCFLAGS=-D warnings`, then `scripts/check_doc_links.py target/doc` | No rustdoc warning, and every link on each crate's front page (its README) is a URL or a file in the docs |
| `test-frontend` | `npm run deadcode`, `npm run test:run` | No TypeScript file that nothing imports, and no unused or undeclared dependency; the frontend tests pass |
| `clippy` | `npm run build`, then `cargo clippy --all-targets --all-features -- -D warnings` | No Clippy warning on Linux |
| `cli-cross-os` | `cargo test -p gglib-cli --no-run` on Linux, macOS and Windows; on Linux and macOS, `gglib-runtime`'s ignored tests against the pinned engines, and the llama-server recording, report-only; on macOS, the in-process engine's recording against the `llama-xcframework` action's framework, report-only | `gglib-cli` and its tests compile on each; every flag gglib passes is one the pinned engines take. The recordings' differences from `contracts/upstream/` show in the step summary without failing the job (see [The engine pins](#the-engine-pins)). When `scope`'s `cross_os` is true |
| `clippy-cross-os` | `npm run build`, then `cargo clippy --all-targets --all-features -- -D warnings` on macOS and Windows | No Clippy warning on either, including in code only that OS compiles. When `scope`'s `cross_os` is true |
| `ios-check` | `make ios-check`, which is `cargo check --locked --target aarch64-apple-ios` over the `Makefile`'s `PHONE_CRATES`, on macOS; `rust-toolchain.toml` installs the target | The crates a phone build compiles compile for iOS (see [Crate Boundaries](#crate-boundaries)). When `scope`'s `ios` is true |
| `scope` | `scripts/ci_scope.py` on Linux, comparing a pull request with its base and a push with the commit `main` was at before it; it runs `scripts/apple/ffi-inputs-changed.sh` | Which macOS and Windows jobs the change can reach. `cross_os`: on a pull request, a change to any path outside the ones the script lists as unable to reach a Rust build; on `main`, a change to `Cargo.lock` or `rust-toolchain.toml`, so the cross-OS caches are saved for the lockfile pull requests restore, or any such change pushed without a pull request. `ios`: the same paths. `apple`: the change reaches `ffi-apple`'s inputs, which are the workspace crates in `gglib-ffi`'s dependency closure as `cargo tree` reads it, `crates/build_common.rs`, `scripts/llama_pin.sh`, the `llama-xcframework` action, the test models' list, the root manifests and lockfile, `rust-toolchain.toml`, the `Makefile`, `scripts/apple/` and `ci.yml`. A change that only moves the version, as a release pull request does, reaches none |
| `ffi-apple` | The `llama-xcframework` action, then `make ffi-xcframework-fast`, `make ffi-binding-check`, `make ffi-smoke` with the recorder's tiny models fetched, on macOS with the runner's default Xcode. When `scope`'s `apple` is true | `GglibFFI.xcframework` builds, and each of its three slices is a dynamic framework for the platform it claims, linking `llama.framework` and no run path of its own; the committed Swift binding is what the library generates; that binding compiles in Swift 6 language mode and calls the macOS slice, whose served engine streams a reply from tinygemma3 carrying the context its watch reports (see [The Swift framework](#the-swift-framework)) |
| `phone-llama` | The `llama-xcframework` action, on macOS: it restores the `llama.xcframework` cached for the pin, the runner's Xcode and the build script, or builds it with `scripts/apple/build-llama-xcframework.sh`, then runs `scripts/apple/check-llama-xcframework.sh`. Then `make engine-llama-test` and `make engine-llama-clippy` against that framework, with the recorder's tiny models fetched as the `pinned-engines` action fetches them | llama.cpp's own `llama.xcframework` builds at the pin with the runner's Xcode, with the iPhone, simulator and Mac slices, each exporting the `llama.h` functions the phone engine calls; the phone engine's C shim compiles against that `llama.h`; its decoder loads stories260K and tinygemma3 on the Mac's slice, streams a greedy reply whose counts and cached prefix are right, stops within a step when aborted and frees what a stopped load read; no Clippy warning in it for the Mac, the iPhone or its simulator (see [The engine pins](#the-engine-pins)). When `scope`'s `apple` is true |

When a CI or Coverage run on `main` completes, `badges.yml` downloads its artifacts and pushes badge JSON to the `badges` branch, where the shields.io badges in crate READMEs read it.

Coverage is measured on every push to `main` with `cargo-llvm-cov` and feeds into the same badge pipeline.

Docs are deployed to GitHub Pages by `release.yml`'s `deploy-docs` job, for each release that is not a prerelease.

---

## Issue & PR Labeling

Every issue and PR needs at least one label from each of four categories: `component:`, `priority:`, `size:`, and `type:`. Most of this is automated — the manual part is smaller than it looks.

### On issues

Opening an issue through GitHub's "New Issue" form (`.github/ISSUE_TEMPLATE/issue.yml`) requires `component:`, `priority:`, `size:`, and `type:` before you can even submit — the fields are required dropdowns, and `component:`/`type:` allow selecting more than one (issues commonly span several crates, or are simultaneously e.g. a bug and a refactor).

If an issue is created outside the form — a scripted `gh issue create`, or the raw API — the form's required fields can't apply, since GitHub only validates them in the web UI and `gh issue create`'s interactive prompts. `issue-labels.yml` is the actual backstop: it checks every issue for all four categories and applies `status: needs-triage` if any are missing, removing it automatically once the labels are added by hand. `is:issue is:open label:"status: needs-triage"` is the live worklist of anything that slipped through.

**Epics:** check the optional "Epic" box on the form (applies `type: epic`) only for an issue that itself tracks multi-phase work. Don't invent a new label per phase — use GitHub's native sub-issues (the "Create sub-issue" button on the epic) to link and order the phases. It gives the epic a live progress checklist for free and has no ceiling on phase count, unlike a per-letter label scheme.

### On PRs

`label-check.yml`'s `enrich` job does three things automatically, before the label check even runs:

- **Inherits `component:`/`priority:`/`type:`** from any issue the PR closes (`Closes #N`, `Fixes #N`, `Resolves #N`, with the number also written as `mmogr/gglib#N` or the issue's URL; a URL with no keyword closes nothing) — skipping any category the PR already carries, so a deliberate override always wins.
- **Auto-applies `component:`** from changed file paths, but only when the touch maps to exactly one component. Backtesting showed this is reliable when unambiguous (~68% match) and wrong most of the time when several components are touched (~63% over-labelled) — so an ambiguous touch gets a comment listing the candidates instead of a guess.
- **Never auto-applies `size:`.** Backtested against 318 merged PRs: line count predicts this repo's `size:` labels only ~49–55% of the time, because `size:` is an hours/days estimate and effort doesn't track diff volume (a 3000-line lockfile-churn PR can be `xs`; a 90-line dead-CSS removal can be `l`, because the work was figuring out what was safe to delete). Instead, the PR gets a comment with the measured diff size and the historical label distribution for PRs of that size, and a human picks.

Two rules apply to a fix:

- **A fix has an issue first.** A PR titled `fix(…):` closes an issue with `Closes #N`, `Fixes #N` or `Resolves #N` (a bare issue URL closes nothing, so it doesn't count), and that issue carries all four categories. Until it does, the assist comment asks for it and `Label Check` fails. The issues are the per-component record of what breaks. A fix with no issue on purpose takes the `no-issue` label instead. A PR exempt from the labels (a bot's, one labelled `automated` or `version-bump`, or one titled `chore(deps):` or `fix(deps):`) is exempt from this too.
- **A fix PR carries `type: bug`**, inherited from its issue like `component:` and `priority:`. There is no `type: fix` label.

In practice: if your PR closes a fully-labelled issue, you'll rarely touch labels on the PR at all — everything but `size:` inherits automatically. `Label Check`'s failure message says exactly which category is missing and why it wasn't auto-filled, or what a fix's issue still lacks. It does not gate merge yet: `main`'s ruleset, ggRules (id 11292806), requires only `CI Success` today, and adding `Label Check` to it is a one-line edit of that ruleset.

`scripts/backtest_label_heuristics.sh` reproduces the accuracy numbers above against current PR history, if the taxonomy or crate layout changes enough to warrant re-checking them.

---

## Pull Request Checklist

Before requesting review, confirm each item:

- [ ] `make pre-commit` passes locally: the Rust, frontend, architecture, bindings and rustdoc checks. It leaves out five CI jobs: the two cross-OS jobs, which run only in CI; `ios-check`, which `make ios-check` runs on macOS with Xcode; `ffi-apple`, which is `make ffi-xcframework-fast ffi-binding-check ffi-smoke` on macOS with Xcode; and `phone-llama`, which `make phone-llama engine-llama-test engine-llama-clippy` runs on macOS with Xcode and CMake.
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
- [ ] A PR based on another PR's branch is joined to it in a GitHub stack ([Stacked pull requests](#stacked-pull-requests)).
