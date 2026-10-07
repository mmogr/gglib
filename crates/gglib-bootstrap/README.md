# gglib-bootstrap

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-complexity.json)

Shared composition root for gglib adapters.

Wiring happens once, here, so every adapter gets its database, repositories,
download manager and `AppCore` built the same way.

This crate consolidates the infrastructure-wiring steps that were previously duplicated
across the CLI, Axum, and Tauri bootstrap modules into a single
`CoreBootstrap::build(config, emitter) → BuiltCore` call.

## Architecture

This crate is the **Composition Root** — sitting between adapter crates and pure infrastructure, wiring all dependencies before handing a fully-configured `BuiltCore` to each adapter.

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 Adapter Layer                                       │
│                  ┌───────────────┐             ┌───────────────┐                    │
│                  │   gglib-axum  │             │   gglib-cli   │                    │
│                  │   (HTTP API)  │             │    (CLI UX)   │                    │
│                  └───────┬───────┘             └───────┬───────┘                    │
│                          │                             │                            │
│                          └──────────────┬──────────────┘                            │
│                                         │                                           │
└─────────────────────────────────────────┼───────────────────────────────────────────┘
                                          ▼
              ┌───────────────────────────────────────────────────────┐
              │              ►► gglib-bootstrap ◄◄                    │
              │        Single shared wiring call for all adapters     │
              └───────────────────────────┬───────────────────────────┘
                                          │
                                          ▼
              ┌───────────────────────────────────────────────────────┐
              │    gglib-core, gglib-db, gglib-download,              │
              │    gglib-gguf, gglib-hf                               │
              │              (Infrastructure crates)                  │
              └───────────────────────────────────────────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## What it wires

1. `SQLite` database pool + repository set
2. `GgufParser` + `ModelRegistrar`
3. `HfClient` (`HuggingFace` HTTP client)
4. Download manager (using the injected `AppEventEmitter`)
5. `AppCore`, with the `ModelVerificationService` it builds from the above,
   whose repair queues on that download manager

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-bootstrap                                     │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                                 │
│   │ config.rs   │  │ built.rs    │  │ builder.rs  │                                 │
│   │BootstrapCfg │  │ BuiltCore   │  │CoreBootstrap│                                 │
│   └─────────────┘  └─────────────┘  └─────────────┘                                 │
│         └───────────────┴────────┬───────┘                                          │
│                                  ▼                                                  │
│                 lib.rs (declares modules + re-exports)                              │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Exported API:** `BootstrapConfig` (input), `CoreBootstrap::build()` (async entry point), `BuiltCore` (output carrying all wired infrastructure).

## Hexagonal boundary

Depends **only** on infrastructure crates:
`gglib-core`, `gglib-db`, `gglib-download`, `gglib-gguf`, `gglib-hf`.

Does **not** depend on adapter crates (`gglib-mcp`, `gglib-axum`, `gglib-tauri`, `gglib-cli`, `gglib-app-services`).

## Testing

The test suite is split into these layers:

| Layer | Location | Purpose |
|-------|----------|---------|
| Repair | `src/builder_repair_tests.rs` | A repair through the wired core queues its download on the manager the adapters hold, and that manager runs it. The Hub is a stand-in. |
| Happy path / config | `tests/build_happy_path.rs` | Full `CoreBootstrap::build()` calls that confirm the wiring succeeds and the returned `BuiltCore` is live. |
| Hub token | `src/builder.rs` `#[cfg(test)]`, `tests/hub_token.rs` | Inline: one token is handed to the Hub client's config, the download manager's config and `AppCore`, or to none of them. `tests/hub_token.rs`: `build()` reads `HF_TOKEN` itself, and the `AppCore` it returns holds it. That test runs itself again in a process started with the variable, since a running process cannot safely set it. |
| Error cases | `tests/build_error_cases.rs` | Exercises the failure paths of `build()` — missing DB directory and DB path pointing at a directory. |
| Functional round-trips | `tests/functional.rs` | End-to-end data round-trips through the wired repositories: model insert/list, settings save/reload, empty-state assertions for downloads, chat history, and MCP servers. |

Shared test helpers (`TempDir`-backed config, `NoopEmitter`, `build_core`) live in
`tests/common/mod.rs` to keep individual test bodies to ≤ 5 lines.

Run all bootstrap tests:

```bash
cargo test -p gglib-bootstrap
```

## Design Principles

1. **No Adapter Dependencies** — Must not depend on tauri, axum, tower, or CLI crates
2. **Single Call** — All infrastructure wired via one `CoreBootstrap::build()` async call
3. **Emitter Injection** — Event emission strategy supplied by the caller (the daemon passes its SSE broadcaster, the CLI a `NoopEmitter`)
4. **Owned Output** — `BuiltCore` owns all constructed values; adapters clone `Arc`s as needed

## Usage

```rust,ignore
use std::sync::Arc;
use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
use gglib_core::paths::{database_path, resolve_models_dir};

let emitter: Arc<dyn AppEventEmitter> = Arc::new(MyAdapterEmitter::new());
let config = BootstrapConfig {
    db_path: database_path()?,
    models_dir: resolve_models_dir(None)?.path,
};
let core = CoreBootstrap::build(config, emitter).await?;
// core.app, core.downloads, core.hf_client … all ready
```

`build()` reads the `HuggingFace` token from `HF_TOKEN` itself
(`gglib_core::hf_token::from_env`, the one place it is read) and hands it to
the Hub client, the download manager and `AppCore`. The config has no field
for it, so an adapter cannot be wired without it.
